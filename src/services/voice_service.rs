use crate::common::*;
use crate::services::media_service::MediaService;
use serde_json::json;

const ALLOWED_AUDIO_TYPES: &[&str] = &[
    "audio/ogg",
    "audio/mp4",
    "audio/mpeg",
    "audio/webm",
    "audio/wav",
    "audio/aac",
    "audio/flac",
];

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
    server_name: String,
}

impl VoiceService {
    pub fn new(media_service: MediaService, server_name: &str) -> Self {
        Self {
            media_service,
            server_name: server_name.to_string(),
        }
    }

    pub fn validate_audio_content_type(content_type: &str) -> Result<(), ApiError> {
        if !ALLOWED_AUDIO_TYPES
            .iter()
            .any(|t| content_type.starts_with(t))
        {
            return Err(ApiError::bad_request(format!(
                "Unsupported audio content type: {}. Allowed: {:?}",
                content_type, ALLOWED_AUDIO_TYPES
            )));
        }
        Ok(())
    }

    pub async fn upload_voice_message(
        &self,
        params: VoiceMessageUploadParams,
    ) -> ApiResult<serde_json::Value> {
        Self::validate_audio_content_type(&params.content_type)?;

        if params.content.len() > MAX_VOICE_SIZE {
            return Err(ApiError::bad_request(format!(
                "Voice message too large: {} bytes (max {} bytes)",
                params.content.len(),
                MAX_VOICE_SIZE
            )));
        }

        let media_result = self
            .media_service
            .upload_media(&params.user_id, &params.content, &params.content_type, None)
            .await?;

        let content_uri = media_result["content_uri"]
            .as_str()
            .unwrap_or_default()
            .to_string();

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

        Ok(json!({
            "content_uri": content_uri,
            "content": voice_content,
            "content_type": params.content_type,
            "duration_ms": params.duration_ms,
            "size": params.content.len()
        }))
    }

    pub async fn get_voice_media(&self, media_id: &str) -> ApiResult<Option<Vec<u8>>> {
        Ok(self
            .media_service
            .get_media(&self.server_name, media_id)
            .await)
    }

    pub async fn delete_voice_media(&self, media_id: &str) -> ApiResult<()> {
        self.media_service
            .delete_media(&self.server_name, media_id)
            .await
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
