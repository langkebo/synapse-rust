use synapse_services::voice_service::VoiceMessageUploadParams;

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
}

// FT-130: register_encrypted_voice service method exists and is callable
// for encrypted room voices uploaded via standard media (application/octet-stream).
#[test]
fn ft130_register_encrypted_voice_signature_exists() {
    use synapse_services::voice_service::VoiceService;
    // Compile-time check: method must be callable with these parameters.
    // Runtime check requires DB; compile check ensures API contract is preserved.
    let _ = || -> Option<fn(&VoiceService, &str, Option<&str>, &str, &str, i32, i64)> {
        Some(VoiceService::register_encrypted_voice)
    };
}
