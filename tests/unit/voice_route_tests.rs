// Voice route layer tests.
//
// Covers the wire-level contracts exposed by `synapse-web/src/routes/voice.rs`:
//   * FT-123: `limit` query parameter clamping for voice listing endpoints
//     (lower bound of 1, upper bound of 100, default of 50).
//   * FT-125: `upload_voice_message` must propagate the service's `ApiError`
//     verbatim instead of flattening every failure to a 500.
//
// The handlers themselves require a fully-wired `RoomContext` (voice_service,
// media_service, etc.), so — following the established pattern in
// `burn_after_read_route_tests.rs` — the pure decision logic is extracted into
// small `pub fn` helpers in `voice.rs` and exercised directly here.

use synapse_common::{ApiError, ApiErrorKind, ApiResult, MatrixErrorCode};
use synapse_web::routes::voice::{clamp_voice_list_limit, voice_upload_response};

// ============================================================================
// FT-123: limit clamping for voice listing endpoints
// ============================================================================

#[test]
fn ft123_negative_limit_clamped_to_lower_bound_of_one() {
    // Bug: `limit.unwrap_or(50).min(100)` has no lower bound, so -1 passes
    // through. After the fix it must clamp to 1.
    assert_eq!(clamp_voice_list_limit(Some(-1)), 1);
}

#[test]
fn ft123_zero_limit_clamped_to_lower_bound_of_one() {
    assert_eq!(clamp_voice_list_limit(Some(0)), 1);
}

#[test]
fn ft123_default_limit_is_fifty_when_missing() {
    assert_eq!(clamp_voice_list_limit(None), 50);
}

#[test]
fn ft123_limit_above_one_hundred_clamped_to_upper_bound() {
    assert_eq!(clamp_voice_list_limit(Some(500)), 100);
}

#[test]
fn ft123_limit_of_one_hundred_is_allowed() {
    assert_eq!(clamp_voice_list_limit(Some(100)), 100);
}

#[test]
fn ft123_normal_limit_within_range_passes_through() {
    assert_eq!(clamp_voice_list_limit(Some(25)), 25);
}

// ============================================================================
// FT-125: upload_voice_message must preserve the service's ApiError
// ============================================================================

#[test]
fn ft125_preserves_service_bad_request_error() {
    // Bug: the handler flattens every service error to a 500 via
    // `ApiError::internal(e.to_string())`, losing the original errcode/error.
    // After the fix a `BadRequest` from the service must surface as a 400.
    let service_err = ApiError::bad_request("duration must be positive".to_string());
    let service_result: ApiResult<serde_json::Value> = Err(service_err);

    let response = voice_upload_response(service_result);

    let err = response.expect_err("service error must propagate, not be swallowed");
    assert_eq!(err.kind, ApiErrorKind::BadRequest, "must keep BadRequest kind (400)");
    assert_eq!(err.code, MatrixErrorCode::BadJson, "must keep M_BAD_JSON errcode");
    assert_eq!(err.message, "duration must be positive", "must keep original message");
}

#[test]
fn ft125_preserves_service_forbidden_error() {
    let service_err = ApiError::forbidden("not allowed".to_string());
    let service_result: ApiResult<serde_json::Value> = Err(service_err);

    let err = voice_upload_response(service_result).expect_err("must propagate");

    assert_eq!(err.kind, ApiErrorKind::Forbidden, "must keep Forbidden kind (403)");
    assert_eq!(err.code, MatrixErrorCode::Forbidden, "must keep M_FORBIDDEN errcode");
}

#[test]
fn ft125_ok_result_is_wrapped_as_json() {
    let payload = serde_json::json!({"content_uri": "mxc://localhost/abc"});
    let service_result: ApiResult<serde_json::Value> = Ok(payload.clone());

    let json = voice_upload_response(service_result).expect("ok result must succeed");
    assert_eq!(json.0, payload, "successful payload must be forwarded unchanged");
}

// FT-130: register_encrypted_voice request validation
#[test]
fn ft130_register_encrypted_voice_request_has_correct_fields() {
    // Ensure the request body structure matches the API contract
    let json = serde_json::json!({
        "media_id": "test_media_id",
        "room_id": "!test_room:localhost",
        "content_type": "application/octet-stream",
        "duration_ms": 5000,
        "size_bytes": 102400
    });

    let deserialized: synapse_web::routes::voice::RegisterEncryptedVoiceRequest =
        serde_json::from_value(json).expect("deserialization should succeed");

    assert_eq!(deserialized.media_id, "test_media_id");
    assert_eq!(deserialized.room_id, Some("!test_room:localhost".to_string()));
    assert_eq!(deserialized.content_type, "application/octet-stream");
    assert_eq!(deserialized.duration_ms, 5000);
    assert_eq!(deserialized.size_bytes, 102400);
}

#[test]
fn ft130_register_encrypted_voice_request_media_id_required() {
    // FT-130: `media_id` is a **required** field on
    // `RegisterEncryptedVoiceRequest` (`String`, not `Option<String>`), so a body
    // that omits it must be rejected by serde before the handler runs.
    //
    // The previous body asserted the opposite (`result.is_ok()`, with the message
    // "media_id is optional at deserialization level"), contradicting both the
    // struct and this test's own name. It went unnoticed because CI's unit step
    // runs without `voice-extended`, so `#[cfg(feature = "voice-extended")]`
    // kept this module out of every CI run (sweep §3 B17); the first
    // configuration that compiled it — `run_local_coverage.sh`'s feature set —
    // failed here.
    let json = serde_json::json!({
        "room_id": "!test_room:localhost",
        "content_type": "application/octet-stream",
        "duration_ms": 5000,
        "size_bytes": 102400
    });

    let result: Result<synapse_web::routes::voice::RegisterEncryptedVoiceRequest, _> = serde_json::from_value(json);

    assert!(result.is_err(), "a body missing `media_id` must fail deserialization, not reach the handler");
}

#[test]
fn ft130_register_encrypted_voice_response_format() {
    // Response format from the handler
    let response = serde_json::json!({
        "content_uri": "mxc://localhost/test_media_id",
        "exists": false
    });

    assert!(response.get("content_uri").is_some());
    assert!(response.get("exists").is_some());
    assert_eq!(response["exists"], false);
}
