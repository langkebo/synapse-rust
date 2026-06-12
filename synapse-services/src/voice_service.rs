use crate::media_service::MediaService;
use serde_json::json;
use synapse_common::*;
use synapse_storage::voice::VoiceStorage;

const ALLOWED_AUDIO_TYPES: &[&str] =
    &["audio/ogg", "audio/mp4", "audio/mpeg", "audio/webm", "audio/wav", "audio/aac", "audio/flac"];

const MAX_VOICE_SIZE: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct VoiceMessageUploadParams {
    pub user_id: String,
    pub room_id: Option<String>,
    pub content: Vec<u8>,
    pub content_type: String,
    pub duration_ms: i32,
    pub waveform: Option<Vec<u16>>,
}

#[derive(Clone)]
pub struct VoiceService {
    media_service: MediaService,
    voice_storage: VoiceStorage,
    server_name: String,
}

impl VoiceService {
    pub fn new(media_service: MediaService, voice_storage: VoiceStorage, server_name: &str) -> Self {
        Self { media_service, voice_storage, server_name: server_name.to_string() }
    }

    pub fn validate_audio_content_type(content_type: &str) -> Result<(), ApiError> {
        if !ALLOWED_AUDIO_TYPES.iter().any(|t| content_type.starts_with(t)) {
            return Err(ApiError::bad_request(format!(
                "Unsupported audio content type: {}. Allowed: {:?}",
                content_type, ALLOWED_AUDIO_TYPES
            )));
        }
        Ok(())
    }

    pub async fn upload_voice_message(&self, params: VoiceMessageUploadParams) -> ApiResult<serde_json::Value> {
        Self::validate_audio_content_type(&params.content_type)?;

        if params.content.len() > MAX_VOICE_SIZE {
            return Err(ApiError::bad_request(format!(
                "Voice message too large: {} bytes (max {} bytes)",
                params.content.len(),
                MAX_VOICE_SIZE
            )));
        }

        let media_result =
            self.media_service.upload_media(&params.user_id, &params.content, &params.content_type, None).await?;

        let content_uri = media_result["content_uri"].as_str().unwrap_or_default().to_string();

        let media_id = synapse_common::media_locator::MediaLocator::parse(&content_uri)
            .map(|loc| loc.media_id)
            .unwrap_or_else(|_| content_uri.trim_start_matches(&format!("mxc://{}/", self.server_name)).to_string());

        let size_bytes = params.content.len() as i64;
        let duration_ms = params.duration_ms;
        let content_type = params.content_type.clone();
        let room_id = params.room_id.clone();
        let user_id = params.user_id.clone();

        let mut voice_content = json!({
            "body": "Voice message",
            "msgtype": "m.audio",
            "url": content_uri,
            "info": {
                "mimetype": params.content_type,
                "size": params.content.len(),
                "duration": params.duration_ms
            },
            "org.matrix.msc3245.voice": {}
        });

        if let Some(waveform) = &params.waveform {
            voice_content["org.matrix.msc3245.voice"]["waveform"] = json!(waveform);
        }

        if let Err(e) = self
            .voice_storage
            .record_upload(&user_id, room_id.as_deref(), &media_id, &content_type, duration_ms, size_bytes)
            .await
        {
            ::tracing::warn!(target: "voice", "Failed to record voice usage stats: {}", e);
        }

        Ok(json!({
            "content_uri": content_uri,
            "content": voice_content,
            "content_type": params.content_type,
            "duration_ms": params.duration_ms,
            "size": params.content.len()
        }))
    }

    pub async fn get_voice_media(&self, media_id: &str) -> ApiResult<Option<Vec<u8>>> {
        Ok(self.media_service.get_media(&self.server_name, media_id).await)
    }

    pub async fn delete_voice_media(&self, media_id: &str) -> ApiResult<()> {
        self.media_service.delete_media(&self.server_name, media_id).await
    }

    pub async fn get_voice_stats(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        match self.voice_storage.get_user_stats(user_id).await {
            Ok(stats) => Ok(json!({
                "user_id": user_id,
                "total_uploads": stats.total_uploads,
                "total_duration_ms": stats.total_duration_ms,
                "total_size_bytes": stats.total_size_bytes,
                "uploads_today": stats.uploads_today,
            })),
            Err(e) => {
                ::tracing::warn!(target: "voice", "Failed to get voice stats: {}", e);
                Ok(json!({
                    "user_id": user_id,
                    "total_uploads": 0,
                    "total_duration_ms": 0,
                    "total_size_bytes": 0,
                    "uploads_today": 0,
                }))
            }
        }
    }

    pub async fn get_room_voice_stats(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        match self.voice_storage.get_room_stats(room_id).await {
            Ok(stats) => Ok(json!({
                "room_id": room_id,
                "total_uploads": stats.total_uploads,
                "total_duration_ms": stats.total_duration_ms,
                "total_size_bytes": stats.total_size_bytes,
            })),
            Err(e) => {
                ::tracing::warn!(target: "voice", "Failed to get room voice stats: {}", e);
                Ok(json!({
                    "room_id": room_id,
                    "total_uploads": 0,
                    "total_duration_ms": 0,
                    "total_size_bytes": 0,
                }))
            }
        }
    }

    pub async fn get_user_voice_stats(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        match self.voice_storage.get_global_user_stats(user_id).await {
            Ok(stats) => Ok(json!({
                "user_id": user_id,
                "total_uploads": stats.total_uploads,
                "total_duration_ms": stats.total_duration_ms,
                "total_size_bytes": stats.total_size_bytes,
                "uploads_today": stats.uploads_today,
            })),
            Err(e) => {
                ::tracing::warn!(target: "voice", "Failed to get user voice stats: {}", e);
                Ok(json!({
                    "user_id": user_id,
                    "total_uploads": 0,
                    "total_duration_ms": 0,
                    "total_size_bytes": 0,
                    "uploads_today": 0,
                }))
            }
        }
    }

    pub async fn get_room_voice_messages(
        &self,
        room_id: &str,
        limit: i64,
        from_ts: Option<i64>,
    ) -> ApiResult<serde_json::Value> {
        let records = self
            .voice_storage
            .get_room_messages(room_id, limit, from_ts)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

        let messages: Vec<serde_json::Value> = records.iter().map(|r| self.record_to_message_json(r)).collect();

        let end_token = messages.last().and_then(|m| m.get("created_ts").and_then(|v| v.as_i64()));

        Ok(json!({
            "room_id": room_id,
            "messages": messages,
            "next_batch": end_token,
        }))
    }

    pub async fn get_user_voice_messages(
        &self,
        user_id: &str,
        limit: i64,
        from_ts: Option<i64>,
    ) -> ApiResult<serde_json::Value> {
        let records = self
            .voice_storage
            .get_user_messages(user_id, limit, from_ts)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;

        let messages: Vec<serde_json::Value> = records.iter().map(|r| self.record_to_message_json(r)).collect();

        let end_token = messages.last().and_then(|m| m.get("created_ts").and_then(|v| v.as_i64()));

        Ok(json!({
            "user_id": user_id,
            "messages": messages,
            "next_batch": end_token,
        }))
    }

    pub async fn get_voice_message_content(&self, media_id: &str) -> ApiResult<serde_json::Value> {
        let record = self
            .voice_storage
            .get_by_media_id(media_id)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?
            .ok_or_else(|| ApiError::not_found(format!("Voice message not found: {}", media_id)))?;

        let content_uri = synapse_common::media_locator::MediaLocator {
            server_name: self.server_name.clone(),
            media_id: media_id.to_string(),
        }
        .to_mxc_url();

        Ok(json!({
            "media_id": record.media_id,
            "user_id": record.user_id,
            "room_id": record.room_id,
            "content_uri": content_uri,
            "content_type": record.content_type,
            "duration_ms": record.duration_ms,
            "size_bytes": record.size_bytes,
            "created_ts": record.created_ts,
        }))
    }

    fn record_to_message_json(&self, record: &synapse_storage::voice::VoiceUsageRecord) -> serde_json::Value {
        let content_uri = synapse_common::media_locator::MediaLocator {
            server_name: self.server_name.clone(),
            media_id: record.media_id.clone(),
        }
        .to_mxc_url();
        json!({
            "media_id": record.media_id,
            "user_id": record.user_id,
            "room_id": record.room_id,
            "content_uri": content_uri,
            "content_type": record.content_type,
            "duration_ms": record.duration_ms,
            "size_bytes": record.size_bytes,
            "created_ts": record.created_ts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_audio_content_type_valid() {
        assert!(VoiceService::validate_audio_content_type("audio/ogg").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/ogg; codecs=opus").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/mp4").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/mpeg").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/webm").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/wav").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/aac").is_ok());
        assert!(VoiceService::validate_audio_content_type("audio/flac").is_ok());
    }

    #[test]
    fn test_validate_audio_content_type_invalid() {
        assert!(VoiceService::validate_audio_content_type("video/mp4").is_err());
        assert!(VoiceService::validate_audio_content_type("image/png").is_err());
        assert!(VoiceService::validate_audio_content_type("text/plain").is_err());
        assert!(VoiceService::validate_audio_content_type("audio/unknown").is_err());
    }

    #[test]
    fn test_upload_params_creation() {
        let params = VoiceMessageUploadParams {
            user_id: "@alice:example.com".to_string(),
            room_id: Some("!room:example.com".to_string()),
            content: vec![0u8; 1024],
            content_type: "audio/ogg".to_string(),
            duration_ms: 5000,
            waveform: Some(vec![100, 200, 300]),
        };

        assert_eq!(params.user_id, "@alice:example.com");
        assert_eq!(params.duration_ms, 5000);
        assert_eq!(params.content.len(), 1024);
        assert!(params.waveform.is_some());
    }

    #[test]
    fn test_upload_params_minimal() {
        let params = VoiceMessageUploadParams {
            user_id: "@bob:example.com".to_string(),
            room_id: None,
            content: vec![1, 2, 3],
            content_type: "audio/ogg".to_string(),
            duration_ms: 1000,
            waveform: None,
        };

        assert!(params.room_id.is_none());
        assert!(params.waveform.is_none());
    }
}
