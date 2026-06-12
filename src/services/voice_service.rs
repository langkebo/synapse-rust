#[cfg(feature = "voice-extended")]
pub use synapse_services::voice_service::*;

#[cfg(test)]
#[cfg(feature = "voice-extended")]
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
