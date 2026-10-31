use super::*;

// -----------------------------------------------------------------------
// ApiError construction and Display
// -----------------------------------------------------------------------

#[test]
fn test_api_error_bad_request_construction() {
    let err = ApiError::bad_request("invalid input");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::BadJson);
    assert_eq!(err.message, "invalid input");
    assert!(err.cause.is_none());
}

#[test]
fn test_api_error_unauthorized_construction() {
    let err = ApiError::unauthorized("bad token");
    assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    assert_eq!(err.code, MatrixErrorCode::Unauthorized);
    assert_eq!(err.message, "bad token");
}

#[test]
fn test_api_error_forbidden_construction() {
    let err = ApiError::forbidden("no access");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::Forbidden);
}

#[test]
fn test_api_error_not_found_construction() {
    let err = ApiError::not_found("missing resource");
    assert_eq!(err.kind, ApiErrorKind::NotFound);
    assert_eq!(err.code, MatrixErrorCode::NotFound);
}

#[test]
fn test_api_error_internal_construction() {
    let err = ApiError::internal("bug");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

#[test]
fn test_api_error_not_implemented_construction() {
    let err = ApiError::not_implemented("not done yet");
    // Normalized to 404: Matrix has no 501 errcode, so unimplemented
    // endpoints are presented as M_UNRECOGNIZED/404.
    assert_eq!(err.kind, ApiErrorKind::NotFound);
    assert_eq!(err.code, MatrixErrorCode::Unimplemented);
    assert!(err.is_not_implemented());
}

// MSC4186: M_UNKNOWN_POS constructor for expired/invalid sliding-sync pos.
#[test]
fn test_api_error_unknown_pos_construction() {
    let err = ApiError::unknown_pos("Invalid or expired position token");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::UnknownPos);
    assert_eq!(err.code.as_str(), "M_UNKNOWN_POS");
    assert_eq!(err.code.http_status(), StatusCode::BAD_REQUEST);
    assert_eq!(err.message, "Invalid or expired position token");
}

// P-048 / B2: M_BAD_PAGINATION constructor for unparseable `since` tokens
// on /sync. Per Matrix spec, the HTTP status must be 400 (Bad Request),
// NOT 401 M_UNKNOWN_TOKEN — the access token is valid; only the
// pagination cursor is bad.
#[test]
fn test_api_error_bad_pagination_construction() {
    let err = ApiError::bad_pagination("Invalid since token");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::BadPagination);
    assert_eq!(err.code.as_str(), "M_BAD_PAGINATION");
    assert_eq!(err.code.http_status(), StatusCode::BAD_REQUEST);
    assert_eq!(err.message, "Invalid since token");
}

#[test]
fn test_matrix_error_code_bad_pagination_round_trip() {
    let json = serde_json::to_string(&MatrixErrorCode::BadPagination).unwrap();
    assert_eq!(json, "\"M_BAD_PAGINATION\"");
    let decoded: MatrixErrorCode = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, MatrixErrorCode::BadPagination);
}

#[test]
fn test_matrix_error_code_unknown_pos_round_trip() {
    let json = serde_json::to_string(&MatrixErrorCode::UnknownPos).unwrap();
    assert_eq!(json, "\"M_UNKNOWN_POS\"");
    let decoded: MatrixErrorCode = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, MatrixErrorCode::UnknownPos);
}

// 审查 #7：M_REQUEST_TIMEOUT 按 Matrix 规范应返回 504（Gateway Timeout），
// 而非 408（Request Timeout，客户端语义）。
#[test]
fn test_request_timeout_maps_to_gateway_timeout_504() {
    let err = ApiError::request_timeout("timed out");
    assert_eq!(err.kind, ApiErrorKind::Timeout);
    assert_eq!(err.code.as_str(), "M_REQUEST_TIMEOUT");
    assert_eq!(err.code.http_status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(err.kind.default_http_status(), StatusCode::GATEWAY_TIMEOUT);
}

#[test]
fn test_api_error_conflict_construction() {
    let err = ApiError::conflict("duplicate");
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    // Generic 409 has no catch-all Matrix errcode → M_UNKNOWN (not M_USER_IN_USE).
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

#[test]
fn test_api_error_conflict_with_construction() {
    let err = ApiError::conflict_with(MatrixErrorCode::RoomInUse, "room in use");
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    assert_eq!(err.code, MatrixErrorCode::RoomInUse);
    assert_eq!(err.message, "room in use");
}

#[test]
fn test_api_error_rate_limited_construction() {
    let err = ApiError::rate_limited("too fast");
    assert_eq!(err.kind, ApiErrorKind::RateLimited);
    assert_eq!(err.code, MatrixErrorCode::LimitExceeded);
    assert_eq!(err.message, "too fast");
    // Empty messages fall back to the generic text.
    assert_eq!(ApiError::rate_limited("").message, "Rate limited");
}

#[test]
fn test_api_error_rate_limited_with_retry_construction() {
    let err = ApiError::rate_limited_with_retry(3000);
    assert_eq!(err.kind, ApiErrorKind::RateLimited);
    assert_eq!(err.code, MatrixErrorCode::LimitExceeded);
    assert_eq!(err.message, "Rate limited");
    assert!(err.cause.is_some());
}

#[test]
fn test_api_error_missing_token() {
    let err = ApiError::missing_token();
    assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    assert_eq!(err.code, MatrixErrorCode::MissingToken);
    assert_eq!(err.message, "Missing access token");
}

#[test]
fn test_api_error_not_json() {
    let err = ApiError::not_json("not json data");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::NotJson);
    assert_eq!(err.message, "not json data");
}

#[test]
fn test_api_error_gone_construction() {
    let err = ApiError::gone("resource deleted");
    assert_eq!(err.kind, ApiErrorKind::Gone);
    assert_eq!(err.code, MatrixErrorCode::NotFound);
}

#[test]
fn test_api_error_authentication() {
    let err = ApiError::authentication("bad credentials");
    assert_eq!(err.kind, ApiErrorKind::Unauthorized);
    assert_eq!(err.code, MatrixErrorCode::UnknownToken);
}

#[test]
fn test_api_error_validation() {
    let err = ApiError::validation("invalid field");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::InvalidParam);
}

#[test]
fn test_api_error_database() {
    let err = ApiError::database("db connection failed");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

#[test]
fn test_api_error_database_with_context_omits_inner_error() {
    // 阶段 0 目标（方案 D1-A）：message 携带操作上下文但不泄漏底层 sqlx 错误细节。
    let err = ApiError::database_with_context("create_megolm_session", &"connection refused");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
    assert_eq!(err.message, "Database error: create_megolm_session");
    assert!(!err.message.contains("connection refused"), "底层错误不应进入 message");
}

/// The masked-message contract must NOT cost us the diagnostic.
///
/// Masking is about what the *client* sees; `cause` is `#[serde(skip)]`, so it
/// never reaches the response body and can safely carry the real error for
/// logs and for `std::error::Error::source()`.
///
/// It used to be dropped entirely: the constructors only handed `err` to
/// `tracing::error!`. In a test binary there is normally no tracing subscriber,
/// so the real database error vanished with no way to recover it — this is how
/// `sync_service::tests::incremental_room_sync_returns_state_delta_not_empty`
/// came to report the unactionable
/// `Internal error: Failed to get room account data, cause: None`.
#[test]
fn context_constructors_preserve_the_inner_error_as_cause() {
    use std::error::Error as _;
    #[derive(Debug)]
    struct Boom;
    impl std::fmt::Display for Boom {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "relation \"room_account_data\" does not exist")
        }
    }
    impl std::error::Error for Boom {}

    let err = ApiError::internal_with_cause("Failed to get room account data", Boom);
    assert_eq!(err.message, "Internal error: Failed to get room account data");
    assert!(!err.message.contains("does not exist"), "inner detail must stay out of message");

    let cause = err.source().expect("cause must be preserved for diagnostics");
    assert!(
        cause.to_string().contains("does not exist"),
        "the real error must be recoverable via Error::source(); got: {cause}"
    );

    // Same for the database flavour.
    let err = ApiError::database_with_cause("create_megolm_session", Boom);
    assert_eq!(err.message, "Database error: create_megolm_session");
    assert!(err.source().is_some(), "database flavour must preserve the cause too");
}

#[test]
fn test_api_error_internal_with_context_omits_inner_error() {
    let err = ApiError::internal_with_context("key_rotation", &"boom");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
    assert_eq!(err.message, "Internal error: key_rotation");
    assert!(!err.message.contains("boom"), "底层错误不应进入 message");
}

#[test]
fn test_map_database_macro_expands() {
    use std::error::Error as _;
    // Uses a real `std::error::Error`, which is what production call sites pass
    // (`sqlx::Error` etc.). The macro now **preserves** that error as the cause,
    // so a bare `&str` payload would no longer typecheck — that is intended:
    // masking the message must not discard the diagnostic.
    #[derive(Debug)]
    struct Db(String);
    impl std::fmt::Display for Db {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
    impl std::error::Error for Db {}

    // arm 3：闭包形式，供 `.map_err(map_database!("..."))` 使用。
    let f = crate::map_database!("create_session");
    let err = f(Db("connection refused".into()));
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.message, "Database error: create_session");
    assert!(!err.message.contains("connection refused"), "detail must stay out of message");
    assert!(err.source().is_some(), "but the detail must survive in the cause");

    // arm 1：`result + literal` 形式。
    let r: Result<(), Db> = Err(Db("boom".into()));
    let mapped = crate::map_database!(r, "load_sessions");
    assert_eq!(mapped.unwrap_err().message, "Database error: load_sessions");
}

#[test]
fn test_api_error_cache() {
    let err = ApiError::cache("redis down");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.message, "Cache error: redis down");
}

// -----------------------------------------------------------------------
// ApiError Display formatting
// -----------------------------------------------------------------------

#[test]
fn test_api_error_display_without_source() {
    let err = ApiError::bad_request("test message");
    assert_eq!(format!("{}", err), "M_BAD_JSON: test message");
}

#[test]
fn test_api_error_display_internal_message() {
    let err = ApiError::internal("something went wrong");
    assert_eq!(format!("{}", err), "M_UNKNOWN: something went wrong");
}

#[test]
fn test_api_error_debug_not_empty() {
    let err = ApiError::forbidden("debug test");
    assert!(!format!("{:?}", err).is_empty());
}

#[test]
fn test_api_error_display_contains_message() {
    let err = ApiError::not_found("resource not found");
    assert!(format!("{}", err).contains("resource not found"));
    assert!(format!("{}", err).contains("M_NOT_FOUND"));
}

// -----------------------------------------------------------------------
// ApiError PartialEq
// -----------------------------------------------------------------------

#[test]
fn test_api_error_partial_eq_equal() {
    let err1 = ApiError::bad_request("msg");
    let err2 = ApiError::bad_request("msg");
    assert_eq!(err1, err2);
}

#[test]
fn test_api_error_partial_eq_different_kind() {
    let err1 = ApiError::bad_request("msg");
    let err2 = ApiError::forbidden("msg");
    assert_ne!(err1, err2);
}

#[test]
fn test_api_error_partial_eq_different_message() {
    let err1 = ApiError::bad_request("msg one");
    let err2 = ApiError::bad_request("msg two");
    assert_ne!(err1, err2);
}

// -----------------------------------------------------------------------
// ApiError builder methods
// -----------------------------------------------------------------------

#[test]
fn test_api_error_with_code() {
    let err = ApiError::bad_request("msg").with_code(MatrixErrorCode::InvalidParam);
    assert_eq!(err.code, MatrixErrorCode::InvalidParam);
}

// -----------------------------------------------------------------------
// ApiError predicate methods
// -----------------------------------------------------------------------

#[test]
fn test_api_error_is_bad_request() {
    assert!(ApiError::bad_request("msg").is_bad_request());
    assert!(!ApiError::forbidden("msg").is_bad_request());
}

#[test]
fn test_api_error_is_unauthorized() {
    assert!(ApiError::unauthorized("msg").is_unauthorized());
    assert!(!ApiError::bad_request("msg").is_unauthorized());
}

#[test]
fn test_api_error_is_forbidden() {
    assert!(ApiError::forbidden("msg").is_forbidden());
    assert!(!ApiError::bad_request("msg").is_forbidden());
}

#[test]
fn test_api_error_is_not_found() {
    assert!(ApiError::not_found("msg").is_not_found());
    assert!(!ApiError::bad_request("msg").is_not_found());
}

#[test]
fn test_api_error_is_conflict() {
    assert!(ApiError::conflict("msg").is_conflict());
    assert!(!ApiError::bad_request("msg").is_conflict());
}

#[test]
fn test_api_error_is_gone() {
    assert!(ApiError::gone("msg").is_gone());
    assert!(!ApiError::bad_request("msg").is_gone());
}

#[test]
fn test_api_error_is_rate_limited() {
    assert!(ApiError::rate_limited("msg").is_rate_limited());
    assert!(!ApiError::bad_request("msg").is_rate_limited());
}

#[test]
fn test_api_error_is_internal() {
    assert!(ApiError::internal("msg").is_internal());
    assert!(!ApiError::bad_request("msg").is_internal());
}

#[test]
fn test_api_error_is_not_implemented() {
    assert!(ApiError::not_implemented("msg").is_not_implemented());
    assert!(!ApiError::bad_request("msg").is_not_implemented());
}

#[test]
fn test_api_error_is_timeout() {
    assert!(ApiError::request_timeout("msg").is_timeout());
    assert!(!ApiError::bad_request("msg").is_timeout());
}

#[test]
fn test_api_error_code_is() {
    let err = ApiError::bad_request("msg");
    assert!(err.code_is(MatrixErrorCode::BadJson));
    assert!(!err.code_is(MatrixErrorCode::NotFound));
}

// -----------------------------------------------------------------------
// ApiError accessor methods
// -----------------------------------------------------------------------

#[test]
fn test_api_error_code_accessor() {
    let err = ApiError::forbidden("msg");
    assert_eq!(err.code(), &MatrixErrorCode::Forbidden);
    assert_eq!(err.matrix_code(), MatrixErrorCode::Forbidden);
    assert_eq!(err.code_str(), "M_FORBIDDEN");
}

#[test]
fn test_api_error_message_for_internal() {
    let err = ApiError::internal("secret details");
    // `message()` masks the internal details for clients
    assert_eq!(err.message(), "An internal error occurred");
}

#[test]
fn test_api_error_message_for_non_internal() {
    let err = ApiError::bad_request("bad input");
    assert_eq!(err.message(), "bad input");
}

#[test]
fn test_api_error_internal_message() {
    let err = ApiError::internal("secret details");
    assert_eq!(err.internal_message(), "secret details");
}

#[test]
fn test_api_error_http_status() {
    let err = ApiError::not_found("gone");
    assert_eq!(err.http_status(), StatusCode::NOT_FOUND);

    let err = ApiError::forbidden("no access");
    assert_eq!(err.http_status(), StatusCode::FORBIDDEN);
}

#[test]
fn test_api_error_retry_after_ms_for_rate_limited_without_cause() {
    let err = ApiError::rate_limited("slow down");
    assert_eq!(err.retry_after_ms(), Some(5000));
}

#[test]
fn test_api_error_retry_after_ms_for_rate_limited_with_retry() {
    let err = ApiError::rate_limited_with_retry(3000);
    assert_eq!(err.retry_after_ms(), Some(3000));
}

#[test]
fn test_api_error_retry_after_ms_for_non_rate_limited() {
    let err = ApiError::bad_request("nope");
    assert_eq!(err.retry_after_ms(), None);
}

#[test]
fn test_api_error_domain_constructors() {
    // user_deactivated
    let err = ApiError::user_deactivated("account disabled");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::UserDeactivated);

    // invalid_username
    let err = ApiError::invalid_username("bad chars");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::InvalidUsername);

    // user_in_use
    let err = ApiError::user_in_use("user taken");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::UserInUse);

    // room_in_use
    let err = ApiError::room_in_use("room in use");
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    assert_eq!(err.code, MatrixErrorCode::RoomInUse);

    // invalid_room_state
    let err = ApiError::invalid_room_state("wrong state");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::InvalidRoomState);

    // threepid_in_use
    let err = ApiError::threepid_in_use("email in use");
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    assert_eq!(err.code, MatrixErrorCode::ThreepidInUse);

    // threepid_not_found
    let err = ApiError::threepid_not_found("email not found");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::ThreepidNotFound);

    // threepid_auth_failed
    let err = ApiError::threepid_auth_failed("auth failed");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::ThreepidAuthFailed);

    // threepid_denied
    let err = ApiError::threepid_denied("denied");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::ThreepidDenied);

    // server_not_trusted
    let err = ApiError::server_not_trusted("untrusted");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::ServerNotTrusted);

    // unsupported_room_version
    let err = ApiError::unsupported_room_version("bad version");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::UnsupportedRoomVersion);

    // incompatible_room_version
    let err = ApiError::incompatible_room_version("wrong version");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::IncompatibleRoomVersion);

    // bad_state
    let err = ApiError::bad_state("bad state");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::BadState);

    // guest_access_forbidden
    let err = ApiError::guest_access_forbidden("guest blocked");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::GuestAccessForbidden);

    // captcha_needed
    let err = ApiError::captcha_needed("captcha required");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::CaptchaNeeded);

    // captcha_invalid
    let err = ApiError::captcha_invalid("wrong captcha");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::CaptchaInvalid);

    // missing_param
    let err = ApiError::missing_param("missing field");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::MissingParam);

    // invalid_param
    let err = ApiError::invalid_param("bad field");
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.code, MatrixErrorCode::InvalidParam);

    // too_large
    let err = ApiError::too_large("too big");
    assert_eq!(err.kind, ApiErrorKind::PayloadTooLarge);
    assert_eq!(err.code, MatrixErrorCode::TooLarge);

    // exclusive
    let err = ApiError::exclusive("exclusive");
    assert_eq!(err.kind, ApiErrorKind::Conflict);
    assert_eq!(err.code, MatrixErrorCode::Exclusive);

    // resource_limit_exceeded
    let err = ApiError::resource_limit_exceeded("over limit");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::ResourceLimitExceeded);

    // cannot_leave_server_notice_room
    let err = ApiError::cannot_leave_server_notice_room("stuck");
    assert_eq!(err.kind, ApiErrorKind::Forbidden);
    assert_eq!(err.code, MatrixErrorCode::CannotLeaveServerNoticeRoom);

    // unknown
    let err = ApiError::unknown("unknown error");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);

    // unrecognized
    let err = ApiError::unrecognized("unrecognized");
    assert_eq!(err.kind, ApiErrorKind::NotFound);
    assert_eq!(err.code, MatrixErrorCode::Unrecognized);

    // request_timeout
    let err = ApiError::request_timeout("timed out");
    assert_eq!(err.kind, ApiErrorKind::Timeout);
    assert_eq!(err.code, MatrixErrorCode::RequestTimeout);

    // decryption_error
    let err = ApiError::decryption_error("decrypt failed");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);

    // encryption_error
    let err = ApiError::encryption_error("encrypt failed");
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.code, MatrixErrorCode::Unknown);
}

// -----------------------------------------------------------------------
// MatrixErrorCode::as_str()
// -----------------------------------------------------------------------

#[test]
fn test_matrix_error_code_as_str() {
    assert_eq!(MatrixErrorCode::Forbidden.as_str(), "M_FORBIDDEN");
    assert_eq!(MatrixErrorCode::Unknown.as_str(), "M_UNKNOWN");
    assert_eq!(MatrixErrorCode::NotFound.as_str(), "M_NOT_FOUND");
    assert_eq!(MatrixErrorCode::BadJson.as_str(), "M_BAD_JSON");
    assert_eq!(MatrixErrorCode::Unauthorized.as_str(), "M_UNAUTHORIZED");
    assert_eq!(MatrixErrorCode::Unimplemented.as_str(), "M_UNRECOGNIZED");
    assert_eq!(MatrixErrorCode::UserDeactivated.as_str(), "M_USER_DEACTIVATED");
    assert_eq!(MatrixErrorCode::LimitExceeded.as_str(), "M_LIMIT_EXCEEDED");
    assert_eq!(MatrixErrorCode::RequestTimeout.as_str(), "M_REQUEST_TIMEOUT");
}

#[test]
fn test_matrix_error_code_as_str_all_variants() {
    // Spot-check that every variant returns a non-empty M_ prefixed string
    let codes = [
        MatrixErrorCode::Forbidden,
        MatrixErrorCode::UnknownToken,
        MatrixErrorCode::MissingToken,
        MatrixErrorCode::BadJson,
        MatrixErrorCode::NotJson,
        MatrixErrorCode::NotFound,
        MatrixErrorCode::LimitExceeded,
        MatrixErrorCode::Unknown,
        MatrixErrorCode::Unrecognized,
        MatrixErrorCode::Unauthorized,
        MatrixErrorCode::UserDeactivated,
        MatrixErrorCode::UserInUse,
        MatrixErrorCode::InvalidUsername,
        MatrixErrorCode::RoomInUse,
        MatrixErrorCode::InvalidRoomState,
        MatrixErrorCode::ThreepidInUse,
        MatrixErrorCode::ThreepidNotFound,
        MatrixErrorCode::ThreepidAuthFailed,
        MatrixErrorCode::ThreepidDenied,
        MatrixErrorCode::ServerNotTrusted,
        MatrixErrorCode::UnsupportedRoomVersion,
        MatrixErrorCode::IncompatibleRoomVersion,
        MatrixErrorCode::BadState,
        MatrixErrorCode::GuestAccessForbidden,
        MatrixErrorCode::CaptchaNeeded,
        MatrixErrorCode::CaptchaInvalid,
        MatrixErrorCode::MissingParam,
        MatrixErrorCode::InvalidParam,
        MatrixErrorCode::TooLarge,
        MatrixErrorCode::Exclusive,
        MatrixErrorCode::ResourceLimitExceeded,
        MatrixErrorCode::CannotLeaveServerNoticeRoom,
        MatrixErrorCode::Unimplemented,
        MatrixErrorCode::RequestTimeout,
        MatrixErrorCode::BadPagination,
    ];
    for code in &codes {
        let s = code.as_str();
        assert!(s.starts_with("M_"), "as_str() for {code:?} did not start with M_: got {s}");
        assert!(!s.is_empty(), "as_str() for {code:?} returned empty string");
    }
}

// -----------------------------------------------------------------------
// MatrixErrorCode::http_status()
// -----------------------------------------------------------------------

#[test]
fn test_matrix_error_code_http_status() {
    assert_eq!(MatrixErrorCode::Forbidden.http_status(), StatusCode::FORBIDDEN);
    assert_eq!(MatrixErrorCode::NotFound.http_status(), StatusCode::NOT_FOUND);
    assert_eq!(MatrixErrorCode::BadJson.http_status(), StatusCode::BAD_REQUEST);
    assert_eq!(MatrixErrorCode::LimitExceeded.http_status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(MatrixErrorCode::Unimplemented.http_status(), StatusCode::NOT_FOUND);
    assert_eq!(MatrixErrorCode::Unrecognized.http_status(), StatusCode::NOT_FOUND);
    assert_eq!(MatrixErrorCode::Unknown.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(MatrixErrorCode::Unauthorized.http_status(), StatusCode::UNAUTHORIZED);
    assert_eq!(MatrixErrorCode::ServerNotTrusted.http_status(), StatusCode::BAD_GATEWAY);
    assert_eq!(MatrixErrorCode::TooLarge.http_status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(MatrixErrorCode::RequestTimeout.http_status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(MatrixErrorCode::UserInUse.http_status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_matrix_error_code_http_status_all_variants() {
    // Verify every variant's http_status matches its expected grouping
    let auth_codes = [MatrixErrorCode::UnknownToken, MatrixErrorCode::MissingToken, MatrixErrorCode::Unauthorized];
    for code in &auth_codes {
        assert_eq!(code.http_status(), StatusCode::UNAUTHORIZED, "{code:?} should be UNAUTHORIZED");
    }

    let bad_request_codes = [
        MatrixErrorCode::BadJson,
        MatrixErrorCode::NotJson,
        MatrixErrorCode::InvalidUsername,
        MatrixErrorCode::UserInUse,
        MatrixErrorCode::InvalidRoomState,
        MatrixErrorCode::UnsupportedRoomVersion,
        MatrixErrorCode::IncompatibleRoomVersion,
        MatrixErrorCode::BadState,
        MatrixErrorCode::CaptchaNeeded,
        MatrixErrorCode::CaptchaInvalid,
        MatrixErrorCode::MissingParam,
        MatrixErrorCode::InvalidParam,
        MatrixErrorCode::ThreepidNotFound,
        MatrixErrorCode::BadPagination,
    ];
    for code in &bad_request_codes {
        assert_eq!(code.http_status(), StatusCode::BAD_REQUEST, "{code:?} should be BAD_REQUEST");
    }

    // COMPAT-03: M_UNRECOGNIZED variants normalize to 404, not 400/501.
    let not_found_codes = [
        MatrixErrorCode::NotFound,
        MatrixErrorCode::Unrecognized,
        MatrixErrorCode::Unimplemented,
        MatrixErrorCode::UnknownDevice,
    ];
    for code in &not_found_codes {
        assert_eq!(code.http_status(), StatusCode::NOT_FOUND, "{code:?} should be NOT_FOUND");
    }

    let forbidden_codes = [
        MatrixErrorCode::Forbidden,
        MatrixErrorCode::UserDeactivated,
        MatrixErrorCode::ThreepidAuthFailed,
        MatrixErrorCode::ThreepidDenied,
        MatrixErrorCode::GuestAccessForbidden,
        MatrixErrorCode::ResourceLimitExceeded,
        MatrixErrorCode::CannotLeaveServerNoticeRoom,
    ];
    for code in &forbidden_codes {
        assert_eq!(code.http_status(), StatusCode::FORBIDDEN, "{code:?} should be FORBIDDEN");
    }

    let conflict_codes = [MatrixErrorCode::RoomInUse, MatrixErrorCode::ThreepidInUse, MatrixErrorCode::Exclusive];
    for code in &conflict_codes {
        assert_eq!(code.http_status(), StatusCode::CONFLICT, "{code:?} should be CONFLICT");
    }
}

// -----------------------------------------------------------------------
// BE-2: HTTP status / errcode divergence guard
// -----------------------------------------------------------------------

/// Every constructor that pins a *fixed* `(kind, code)` pair, as
/// `(name, ApiError)`.
///
/// `conflict_with` and the `with_code` builder are deliberately excluded:
/// they are open escape hatches that accept an arbitrary errcode.
///
/// NB: when adding an `ApiError` constructor, add it here too — this list is
/// exactly what the divergence guard iterates over.
fn all_fixed_api_error_constructors() -> Vec<(&'static str, ApiError)> {
    vec![
        ("bad_request", ApiError::bad_request("x")),
        ("bad_request_unknown", ApiError::bad_request_unknown("x")),
        ("unknown_pos", ApiError::unknown_pos("x")),
        ("bad_pagination", ApiError::bad_pagination("x")),
        ("key_too_large", ApiError::key_too_large("x")),
        ("profile_too_large", ApiError::profile_too_large("x")),
        ("unauthorized", ApiError::unauthorized("x")),
        ("invalid_credentials", ApiError::invalid_credentials()),
        ("forbidden", ApiError::forbidden("x")),
        ("not_found", ApiError::not_found("x")),
        ("not_implemented", ApiError::not_implemented("x")),
        ("conflict", ApiError::conflict("x")),
        ("internal", ApiError::internal("x")),
        ("service_unavailable", ApiError::service_unavailable("x")),
        ("content_scan_failed", ApiError::content_scan_failed("x")),
        ("content_scan_disabled", ApiError::content_scan_disabled("x")),
        ("fed_proxy_destination_denied", ApiError::fed_proxy_destination_denied("x")),
        ("fed_proxy_path_not_allowed", ApiError::fed_proxy_path_not_allowed("x")),
        ("fed_proxy_connection_failed", ApiError::fed_proxy_connection_failed("x")),
        ("not_supported", ApiError::not_supported("x")),
        ("database_with_context", ApiError::database_with_context("x", &"y")),
        ("internal_with_context", ApiError::internal_with_context("x", &"y")),
        ("database", ApiError::database("x")),
        ("database_with_cause", ApiError::database_with_cause("x", io_err())),
        ("internal_with_cause", ApiError::internal_with_cause("x", io_err())),
        ("internal_with_boxed_cause", ApiError::internal_with_boxed_cause("x", Box::new(io_err()))),
        ("cache", ApiError::cache("x")),
        ("gone", ApiError::gone("x")),
        ("authentication", ApiError::authentication("x")),
        ("validation", ApiError::validation("x")),
        ("invalid_input", ApiError::invalid_input("x")),
        ("crypto", ApiError::crypto("x")),
        ("rate_limited", ApiError::rate_limited("x")),
        ("rate_limited_with_retry", ApiError::rate_limited_with_retry(1000)),
        ("missing_token", ApiError::missing_token()),
        ("not_json", ApiError::not_json("x")),
        ("user_deactivated", ApiError::user_deactivated("x")),
        ("invalid_username", ApiError::invalid_username("x")),
        ("user_in_use", ApiError::user_in_use("x")),
        ("room_in_use", ApiError::room_in_use("x")),
        ("invalid_room_state", ApiError::invalid_room_state("x")),
        ("threepid_in_use", ApiError::threepid_in_use("x")),
        ("threepid_not_found", ApiError::threepid_not_found("x")),
        ("threepid_auth_failed", ApiError::threepid_auth_failed("x")),
        ("threepid_denied", ApiError::threepid_denied("x")),
        ("server_not_trusted", ApiError::server_not_trusted("x")),
        ("unsupported_room_version", ApiError::unsupported_room_version("x")),
        ("incompatible_room_version", ApiError::incompatible_room_version("x")),
        ("bad_state", ApiError::bad_state("x")),
        ("guest_access_forbidden", ApiError::guest_access_forbidden("x")),
        ("captcha_needed", ApiError::captcha_needed("x")),
        ("captcha_invalid", ApiError::captcha_invalid("x")),
        ("missing_param", ApiError::missing_param("x")),
        ("invalid_param", ApiError::invalid_param("x")),
        ("too_large", ApiError::too_large("x")),
        ("exclusive", ApiError::exclusive("x")),
        ("resource_limit_exceeded", ApiError::resource_limit_exceeded("x")),
        ("cannot_leave_server_notice_room", ApiError::cannot_leave_server_notice_room("x")),
        ("unknown", ApiError::unknown("x")),
        ("unrecognized", ApiError::unrecognized("x")),
        ("request_timeout", ApiError::request_timeout("x")),
        ("decryption_error", ApiError::decryption_error("x")),
        ("encryption_error", ApiError::encryption_error("x")),
    ]
}

fn io_err() -> std::io::Error {
    std::io::Error::other("y")
}

fn is_registered_status_divergence(kind: ApiErrorKind, code: MatrixErrorCode) -> bool {
    STATUS_DIVERGENCE_WHITELIST.iter().any(|(k, c)| *k == kind && *c == code)
}

/// `ApiError::into_response` derives the HTTP status from `kind` while the
/// errcode comes from `code`. Any constructor whose `kind` status differs
/// from its `code` spec status is a "divergence" and must be registered in
/// `STATUS_DIVERGENCE_WHITELIST`.
#[test]
fn test_api_error_status_divergences_are_registered() {
    let mut unregistered = Vec::new();
    for (name, err) in all_fixed_api_error_constructors() {
        let kind_status = err.kind.default_http_status();
        let code_status = err.code.http_status();
        if kind_status != code_status && !is_registered_status_divergence(err.kind, err.code) {
            unregistered.push(format!("{name}: kind={:?}({kind_status}) code={:?}({code_status})", err.kind, err.code));
        }
    }
    assert!(
        unregistered.is_empty(),
        "unregistered HTTP status divergence(s) — fix the constructor or add to STATUS_DIVERGENCE_WHITELIST:\n{}",
        unregistered.join("\n")
    );
}

/// The registry must not rot: every entry must be both *reachable* through a
/// constructor and *actually divergent*. A stale entry is a bug.
#[test]
fn test_status_divergence_whitelist_has_no_stale_entries() {
    let observed: Vec<(ApiErrorKind, MatrixErrorCode)> = all_fixed_api_error_constructors()
        .into_iter()
        .filter(|(_, err)| err.kind.default_http_status() != err.code.http_status())
        .map(|(_, err)| (err.kind, err.code))
        .collect();

    for entry in STATUS_DIVERGENCE_WHITELIST {
        assert!(
                observed.contains(entry),
                "STATUS_DIVERGENCE_WHITELIST entry {entry:?} is stale — no constructor produces it (or it no longer diverges)"
            );
    }
    for pair in &observed {
        assert!(
            is_registered_status_divergence(pair.0, pair.1),
            "constructor divergence {pair:?} is missing from STATUS_DIVERGENCE_WHITELIST"
        );
    }
}

/// Mutation-check (铁律 8): prove the guard can actually fail. A real
/// divergence nobody registered must be rejected; a registered one accepted.
#[test]
fn test_status_divergence_guard_can_fail() {
    // Forbidden(403) + M_NOT_FOUND(404) is a genuine divergence, unregistered.
    assert!(!is_registered_status_divergence(ApiErrorKind::Forbidden, MatrixErrorCode::NotFound));
    // Sanity: a registered divergence is accepted.
    assert!(is_registered_status_divergence(ApiErrorKind::Gone, MatrixErrorCode::NotFound));
}

// -----------------------------------------------------------------------
// ApiResponse::into_response 与 MatrixErrorCode::http_status 收敛一致性
// -----------------------------------------------------------------------

/// 全量 errcode 变体：ApiResponse 的 errcode→状态码必须与
/// MatrixErrorCode::http_status() 完全一致，防止第三套硬编码映射漂移。
fn all_error_codes() -> Vec<MatrixErrorCode> {
    vec![
        MatrixErrorCode::Forbidden,
        MatrixErrorCode::UnknownToken,
        MatrixErrorCode::MissingToken,
        MatrixErrorCode::BadJson,
        MatrixErrorCode::NotJson,
        MatrixErrorCode::NotFound,
        MatrixErrorCode::LimitExceeded,
        MatrixErrorCode::Unknown,
        MatrixErrorCode::Unrecognized,
        MatrixErrorCode::Unauthorized,
        MatrixErrorCode::UserDeactivated,
        MatrixErrorCode::UserInUse,
        MatrixErrorCode::InvalidUsername,
        MatrixErrorCode::RoomInUse,
        MatrixErrorCode::InvalidRoomState,
        MatrixErrorCode::ThreepidInUse,
        MatrixErrorCode::ThreepidNotFound,
        MatrixErrorCode::ThreepidAuthFailed,
        MatrixErrorCode::ThreepidDenied,
        MatrixErrorCode::ServerNotTrusted,
        MatrixErrorCode::UnsupportedRoomVersion,
        MatrixErrorCode::IncompatibleRoomVersion,
        MatrixErrorCode::BadState,
        MatrixErrorCode::GuestAccessForbidden,
        MatrixErrorCode::CaptchaNeeded,
        MatrixErrorCode::CaptchaInvalid,
        MatrixErrorCode::MissingParam,
        MatrixErrorCode::InvalidParam,
        MatrixErrorCode::TooLarge,
        MatrixErrorCode::Exclusive,
        MatrixErrorCode::ResourceLimitExceeded,
        MatrixErrorCode::CannotLeaveServerNoticeRoom,
        MatrixErrorCode::Unimplemented,
        MatrixErrorCode::RequestTimeout,
        MatrixErrorCode::UserLimitExceeded,
        MatrixErrorCode::UnknownPos,
        MatrixErrorCode::KeyTooLarge,
        MatrixErrorCode::ProfileTooLarge,
        MatrixErrorCode::BadPagination,
    ]
}

#[test]
fn test_api_response_status_covers_previously_missing_errcodes() {
    use axum::response::IntoResponse;
    // 之前 ApiResponse::into_response 的硬编码 match 缺失这些 errcode（落 500），
    // 收敛到 MatrixErrorCode::from_str + http_status() 后应正确。
    let cases: &[(&str, StatusCode)] = &[
        ("M_REQUEST_TIMEOUT", StatusCode::GATEWAY_TIMEOUT),
        ("M_UNKNOWN_POS", StatusCode::BAD_REQUEST),
        ("M_USER_LIMIT_EXCEEDED", StatusCode::TOO_MANY_REQUESTS),
        ("M_USER_DEACTIVATED", StatusCode::FORBIDDEN),
        ("M_GUEST_ACCESS_FORBIDDEN", StatusCode::FORBIDDEN),
        ("M_RESOURCE_LIMIT_EXCEEDED", StatusCode::FORBIDDEN),
        ("M_CANNOT_LEAVE_SERVER_NOTICE_ROOM", StatusCode::FORBIDDEN),
        ("M_THREEPID_AUTH_FAILED", StatusCode::FORBIDDEN),
        ("M_THREEPID_DENIED", StatusCode::FORBIDDEN),
        ("M_THREEPID_NOT_FOUND", StatusCode::BAD_REQUEST),
        ("M_EXCLUSIVE", StatusCode::CONFLICT),
        ("M_UNSUPPORTED_ROOM_VERSION", StatusCode::BAD_REQUEST),
        ("M_INCOMPATIBLE_ROOM_VERSION", StatusCode::BAD_REQUEST),
        ("M_UNRECOGNIZED", StatusCode::NOT_FOUND),
    ];
    for (errcode, expected) in cases {
        let resp = ApiResponse::<serde_json::Value>::error("x".to_string(), (*errcode).to_string());
        assert_eq!(resp.into_response().status(), *expected, "errcode={errcode}");
    }
}

#[test]
fn test_api_response_unknown_errcode_falls_back_to_internal_error() {
    use axum::response::IntoResponse;
    let resp = ApiResponse::<serde_json::Value>::error("x".to_string(), "M_NOT_A_REAL_CODE".to_string());
    assert_eq!(resp.into_response().status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn test_api_response_status_converges_with_matrix_error_code_http_status() {
    use axum::response::IntoResponse;
    // 遍历所有唯一 errcode 字符串，ApiResponse 状态码必须等于
    // MatrixErrorCode::from_str → http_status() 单一映射源。
    // 注意 Unimplemented 与 Unrecognized 共享 "M_UNRECOGNIZED"（by-design，
    // 见 as_str），故按唯一 errcode 去重后以 from_str 结果为准。
    let mut seen = std::collections::HashSet::new();
    for code in all_error_codes() {
        let errcode = code.as_str().to_string();
        if !seen.insert(errcode.clone()) {
            continue;
        }
        let resp = ApiResponse::<serde_json::Value>::error("x".to_string(), errcode.clone());
        let status = resp.into_response().status();
        let expected =
            MatrixErrorCode::from_str(&errcode).map(|c| c.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            status, expected,
            "ApiResponse 状态码 {status} 与 from_str({errcode})→http_status {expected:?} 漂移"
        );
    }
}

/// as_str 与 from_str 互逆性：新增变体时若 from_str 漏加字符串映射，
/// 此测试会在 round-trip 处失败。Unimplemented 与 Unrecognized 共享
/// "M_UNRECOGNIZED"（by-design），单独跳过。
#[test]
fn test_matrix_error_code_as_str_round_trips_through_from_str() {
    for code in all_error_codes() {
        let s = code.as_str();
        let parsed = MatrixErrorCode::from_str(s);
        assert!(parsed.is_some(), "as_str({s}) 无法被 from_str 解析，from_str 漏了映射（{code:?}）");
        if code == MatrixErrorCode::Unimplemented {
            continue;
        }
        assert_eq!(parsed.unwrap(), code, "as_str→from_str round-trip 不对称（{code:?}）");
    }
}

// -----------------------------------------------------------------------
// MatrixErrorCode Display
// -----------------------------------------------------------------------

#[test]
fn test_matrix_error_code_display() {
    assert_eq!(format!("{}", MatrixErrorCode::Forbidden), "M_FORBIDDEN");
    assert_eq!(format!("{}", MatrixErrorCode::NotFound), "M_NOT_FOUND");
    assert_eq!(format!("{}", MatrixErrorCode::Unknown), "M_UNKNOWN");
}

// -----------------------------------------------------------------------
// ApiErrorKind
// -----------------------------------------------------------------------

#[test]
fn test_api_error_kind_default_http_status() {
    assert_eq!(ApiErrorKind::BadRequest.default_http_status(), StatusCode::BAD_REQUEST);
    assert_eq!(ApiErrorKind::Unauthorized.default_http_status(), StatusCode::UNAUTHORIZED);
    assert_eq!(ApiErrorKind::Forbidden.default_http_status(), StatusCode::FORBIDDEN);
    assert_eq!(ApiErrorKind::NotFound.default_http_status(), StatusCode::NOT_FOUND);
    assert_eq!(ApiErrorKind::Conflict.default_http_status(), StatusCode::CONFLICT);
    assert_eq!(ApiErrorKind::Gone.default_http_status(), StatusCode::GONE);
    assert_eq!(ApiErrorKind::PayloadTooLarge.default_http_status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(ApiErrorKind::RateLimited.default_http_status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(ApiErrorKind::Internal.default_http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(ApiErrorKind::NotImplemented.default_http_status(), StatusCode::NOT_IMPLEMENTED);
    assert_eq!(ApiErrorKind::Timeout.default_http_status(), StatusCode::GATEWAY_TIMEOUT);
}

#[test]
fn test_api_error_kind_serde_roundtrip() {
    let variants = [
        ApiErrorKind::BadRequest,
        ApiErrorKind::Unauthorized,
        ApiErrorKind::Forbidden,
        ApiErrorKind::NotFound,
        ApiErrorKind::Conflict,
        ApiErrorKind::Gone,
        ApiErrorKind::PayloadTooLarge,
        ApiErrorKind::RateLimited,
        ApiErrorKind::Internal,
        ApiErrorKind::NotImplemented,
        ApiErrorKind::Timeout,
    ];
    for variant in &variants {
        let json = serde_json::to_string(variant).expect("serialize");
        let deserialized: ApiErrorKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*variant, deserialized, "roundtrip failed for {variant:?}");
    }
}

// -----------------------------------------------------------------------
// Send + Sync (required by Axum)
// -----------------------------------------------------------------------

#[test]
fn test_error_types_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ApiError>();
    assert_send_sync::<MatrixErrorCode>();
    assert_send_sync::<ApiErrorKind>();
    assert_send_sync::<ApiResponse<String>>();
}

// -----------------------------------------------------------------------
// From impls
// -----------------------------------------------------------------------

#[test]
fn test_from_serde_json_error() {
    let serde_err = serde_json::from_str::<i32>("not a number").unwrap_err();
    let api_err: ApiError = serde_err.into();
    assert_eq!(api_err.kind, ApiErrorKind::BadRequest);
    assert_eq!(api_err.code, MatrixErrorCode::NotJson);
}

#[test]
fn test_from_parse_int_error() {
    let parse_err = "abc".parse::<i32>().unwrap_err();
    let api_err: ApiError = parse_err.into();
    assert_eq!(api_err.kind, ApiErrorKind::BadRequest);
    assert_eq!(api_err.code, MatrixErrorCode::InvalidParam);
}

#[test]
fn test_from_utf8_error() {
    let bytes = vec![0xFF, 0xFE, 0x00];
    let utf8_err = String::from_utf8(bytes).unwrap_err();
    let api_err: ApiError = utf8_err.into();
    assert_eq!(api_err.kind, ApiErrorKind::BadRequest);
    assert_eq!(api_err.code, MatrixErrorCode::InvalidParam);
    assert_eq!(api_err.message, "Invalid UTF-8 encoding");
}

// -----------------------------------------------------------------------
// ApiResponse
// -----------------------------------------------------------------------

#[test]
fn test_api_response_success() {
    let resp: ApiResponse<i32> = ApiResponse::success(42);
    assert_eq!(resp.status, "ok");
    assert_eq!(resp.data, Some(42));
    assert!(resp.error.is_none());
    assert!(resp.errcode.is_none());
    assert!(resp.retry_after_ms.is_none());
}

#[test]
fn test_api_response_error() {
    let resp: ApiResponse<()> = ApiResponse::error("bad things".into(), "M_UNKNOWN".into());
    assert_eq!(resp.status, "error");
    assert!(resp.data.is_none());
    assert_eq!(resp.error.as_deref(), Some("bad things"));
    assert_eq!(resp.errcode.as_deref(), Some("M_UNKNOWN"));
    assert!(resp.retry_after_ms.is_none());
}

#[test]
fn test_api_response_error_with_retry() {
    let resp: ApiResponse<()> = ApiResponse::error_with_retry("rate limited".into(), "M_LIMIT_EXCEEDED".into(), 5000);
    assert_eq!(resp.status, "error");
    assert!(resp.data.is_none());
    assert_eq!(resp.error.as_deref(), Some("rate limited"));
    assert_eq!(resp.errcode.as_deref(), Some("M_LIMIT_EXCEEDED"));
    assert_eq!(resp.retry_after_ms, Some(5000));
}

// -----------------------------------------------------------------------
// From<sqlx::Error> structured unique-violation classification (#19)
// -----------------------------------------------------------------------

/// A mock sqlx database error whose SQLSTATE is a unique violation (23505)
/// but whose display/debug text deliberately does NOT contain the magic
/// substrings ("duplicate key", "unique constraint", "23505", ...) the old
/// string-matching logic relied on. This proves classification is driven by
/// the structured error kind, not by text matching.
#[derive(Debug)]
struct MockUniqueViolationError;

impl std::fmt::Display for MockUniqueViolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("boom")
    }
}

impl std::error::Error for MockUniqueViolationError {}

impl sqlx::error::DatabaseError for MockUniqueViolationError {
    fn message(&self) -> &str {
        "boom"
    }

    fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some("23505".into())
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::UniqueViolation
    }
}

#[test]
fn test_from_sqlx_unique_violation_is_structured_not_string_matched() {
    let err: ApiError = sqlx::Error::Database(Box::new(MockUniqueViolationError)).into();
    assert_eq!(err.kind, ApiErrorKind::BadRequest);
    assert_eq!(err.message, "A duplicate entry was found");
}

#[test]
fn test_from_sqlx_row_not_found_maps_to_database_error() {
    let err: ApiError = sqlx::Error::RowNotFound.into();
    assert_eq!(err.kind, ApiErrorKind::Internal);
    assert_eq!(err.message, "A database error occurred");
}

/// ARCH-03「未实现语义表」的可执行版本 —— 契约文档
/// `docs/synapse-rust/ROUTE_CONTRACT.md` 的同名表就是这三行。
///
/// 三者必须**同时**不同：把 `not_supported` 改回 404、或把 `internal` 映射成 501，
/// 都会让"端点不存在 / 端点存在但不支持 / 服务器坏了"重新混成一套语义。
#[test]
fn arch03_unimplemented_semantics_table_is_pinned() {
    use axum::http::StatusCode;

    let not_provided = ApiError::not_implemented("endpoint not provided");
    // 注意 wire 值：`MatrixErrorCode::Unimplemented` 与 `Unrecognized` **共用**同一个
    // 线上字符串 `M_UNRECOGNIZED`（见 `MatrixErrorCode::as_str` 的既有用例），
    // 内部靠 `is_not_implemented()` 区分。契约只承诺 wire 值，所以这里断言 wire 值。
    assert_eq!(not_provided.code.as_str(), "M_UNRECOGNIZED", "404 配 M_UNRECOGNIZED（端点/能力整体不提供）");
    assert_eq!(not_provided.http_status(), StatusCode::NOT_FOUND);
    assert!(not_provided.is_not_implemented(), "「未实现」的内部标记必须保留，否则运维侧分不清缺端点与打错路径");

    let field_unsupported = ApiError::not_supported("erase=true is not supported");
    assert_eq!(field_unsupported.code, MatrixErrorCode::Unknown, "501 配 M_UNKNOWN（端点可用，字段/扩展被拒）");
    assert_eq!(field_unsupported.http_status(), StatusCode::NOT_IMPLEMENTED);

    // 404 与 501 是两个不同状态码，ARCH-03 明确禁止合并。
    assert_ne!(
        not_provided.http_status(),
        field_unsupported.http_status(),
        "404 与 501 的区分是刻意的：合并后客户端分不清「换端点」还是「去掉某个字段」"
    );
    assert_ne!(not_provided.code.as_str(), field_unsupported.code.as_str());

    // 反向：真实内部错误不能被伪装成 404/501。
    let internal = ApiError::internal("database unavailable");
    assert_eq!(internal.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(internal.code.as_str(), "M_UNKNOWN");
    assert!(!internal.is_not_implemented());
    assert_ne!(internal.http_status(), not_provided.http_status());
    assert_ne!(internal.http_status(), field_unsupported.http_status());
}
