#![cfg(test)]
#![cfg(feature = "voice-extended")]

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use synapse_rust::services::media::MediaDomainService;
use synapse_rust::services::media::chunked_upload::ChunkedUploadService;
use synapse_rust::services::media_quota_service::MediaQuotaService;
use synapse_rust::services::voice_service::{VoiceMessageUploadParams, VoiceService};
use synapse_rust::storage::media_quota::MediaQuotaStorage;
use synapse_rust::storage::voice::VoiceStorage;

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

#[tokio::test]
async fn test_voice_service_new() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let media_path = temp_dir.path().to_str().unwrap();
    let pool = Arc::new(
        PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy("postgres://localhost/test")
            .unwrap(),
    );
    let media_service =
        synapse_rust::services::media_service::MediaService::new(media_path, None, "test.server");
    let media_quota_service = Arc::new(MediaQuotaService::new(Arc::new(MediaQuotaStorage::new(&pool))));
    let chunked_upload_service = Arc::new(ChunkedUploadService::new(pool.clone()));
    let media_domain_service = Arc::new(MediaDomainService::new(
        media_service,
        media_quota_service,
        chunked_upload_service,
    ));
    let voice_storage = VoiceStorage::new(pool);
    let _voice_service = VoiceService::new(media_domain_service, voice_storage, "test.server");
}
