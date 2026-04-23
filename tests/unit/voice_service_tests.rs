#![cfg(test)]

use synapse_rust::services::voice_service::{VoiceMessageUploadParams, VoiceService};

#[test]
fn test_voice_upload_params_creation() {
    let params = VoiceMessageUploadParams {
        user_id: "@alice:localhost".to_string(),
        room_id: Some("!room:localhost".to_string()),
        content: vec![0u8; 1024],
        content_type: "audio/ogg".to_string(),
        duration_ms: 5000,
        waveform: Some(vec![100, 200, 300]),
    };

    assert_eq!(params.user_id, "@alice:localhost");
    assert_eq!(params.duration_ms, 5000);
    assert_eq!(params.content.len(), 1024);
    assert!(params.waveform.is_some());
    assert_eq!(params.waveform.as_ref().unwrap().len(), 3);
}

#[test]
fn test_voice_upload_params_minimal() {
    let params = VoiceMessageUploadParams {
        user_id: "@bob:localhost".to_string(),
        room_id: None,
        content: vec![1, 2, 3],
        content_type: "audio/ogg".to_string(),
        duration_ms: 1000,
        waveform: None,
    };

    assert!(params.room_id.is_none());
    assert!(params.waveform.is_none());
    assert_eq!(params.content.len(), 3);
}

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
    assert!(VoiceService::validate_audio_content_type("").is_err());
}

#[test]
fn test_voice_service_new() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let media_path = temp_dir.path().to_str().unwrap();
    let media_service =
        synapse_rust::services::media_service::MediaService::new(media_path, None, "test.server");
    let _voice_service = VoiceService::new(media_service, "test.server");
}
