//! `ApiError` + `MatrixErrorCode`: the unified HTTP/JSON error envelope for all handlers.

use std::sync::{Arc, OnceLock};

use axum::{
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::metrics::MetricsCollector;

static ERROR_METRICS: OnceLock<Arc<MetricsCollector>> = OnceLock::new();

/// Initialise the global error-metrics collector used by `ApiError::into_response`.
pub fn init_error_metrics(collector: Arc<MetricsCollector>) {
    let _ = ERROR_METRICS.set(collector);
}

mod code;

pub use code::MatrixErrorCode;

// ApiErrorKind — semantic error category (10 variants replacing 42)
// ---------------------------------------------------------------------------

/// HTTP semantic category that drives the response status. Serializes as snake_case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorKind {
    /// 400 — client request is malformed
    BadRequest,
    /// 401 — authentication required or invalid
    Unauthorized,
    /// 403 — authenticated but not allowed
    Forbidden,
    /// 404 — resource does not exist
    NotFound,
    /// 409 — resource conflict (e.g. duplicate)
    Conflict,
    /// 410 — resource permanently gone
    Gone,
    /// 413 — request body exceeds size limit (M_TOO_LARGE)
    PayloadTooLarge,
    /// 429 — rate limit exceeded
    RateLimited,
    /// 500 — unexpected internal error
    Internal,
    /// 501 — not implemented
    NotImplemented,
    /// 502 — an upstream dependency failed (e.g. the content-scanner webhook or
    /// ClamAV daemon could not produce a verdict)
    BadGateway,
    /// 504 — request timed out
    Timeout,
    /// 503 — service temporarily unavailable (e.g. required dependency offline)
    ServiceUnavailable,
}

impl ApiErrorKind {
    /// Maps this kind to its canonical HTTP status code.
    pub fn default_http_status(&self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::Gone => StatusCode::GONE,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::BadGateway => StatusCode::BAD_GATEWAY,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP status divergence registry (BE-2)
// ---------------------------------------------------------------------------

/// `(kind, code)` pairs where a constructor **intentionally** sets an HTTP
/// status (via [`ApiErrorKind::default_http_status`]) that differs from the
/// Matrix-spec status implied by the errcode ([`MatrixErrorCode::http_status`]).
///
/// The single source of truth for an `ApiError`'s HTTP status is its `kind`
/// (see `ApiError::into_response`); `code` only fills the body's `errcode`
/// field. The two are set independently, so a divergence is legal — but it must
/// be *known*. Every divergence reachable through an `ApiError` constructor is
/// listed here with a reason; the guard test
/// `test_api_error_status_divergences_are_registered` fails on any unregistered
/// divergence, and `test_status_divergence_whitelist_has_no_stale_entries`
/// fails if an entry here stops being reachable.
pub const STATUS_DIVERGENCE_WHITELIST: &[(ApiErrorKind, MatrixErrorCode)] = &[
    // `gone()`: 410 Gone. Matrix defines no M_GONE, so the errcode is
    // M_NOT_FOUND (whose spec status is 404).
    (ApiErrorKind::Gone, MatrixErrorCode::NotFound),
    // `service_unavailable()`: 503 with no dedicated Matrix errcode → M_UNKNOWN
    // (whose spec status is 500).
    (ApiErrorKind::ServiceUnavailable, MatrixErrorCode::Unknown),
    // `server_not_trusted()`: 403 Forbidden to the caller. M_SERVER_NOT_TRUSTED's
    // canonical status is 502, which is only meaningful on the federation hop.
    (ApiErrorKind::Forbidden, MatrixErrorCode::ServerNotTrusted),
    // `conflict()`: generic 409 conflict with no catch-all Matrix errcode →
    // M_UNKNOWN (whose spec status is 500).
    (ApiErrorKind::Conflict, MatrixErrorCode::Unknown),
    // `not_supported()`: 501 Not Implemented for a known-but-unimplemented
    // extension field (e.g. an MSC field this server refuses to silently drop).
    // Matrix defines no errcode for 501 → M_UNKNOWN (whose spec status is 500).
    (ApiErrorKind::NotImplemented, MatrixErrorCode::Unknown),
];

// ---------------------------------------------------------------------------
// ApiError — structured error with kind / code / cause
// ---------------------------------------------------------------------------

/// Type alias for an opaque, cloneable underlying error cause carried by `ApiError`.
pub type ApiErrorCause = Arc<dyn std::error::Error + Send + Sync>;

/// Error types that can be preserved as an [`ApiError::cause`].
///
/// Implemented for everything `std::error::Error + Send + Sync + 'static`.
/// Notably this includes `Box<dyn Error + Send + Sync>`, which this toolchain
/// already implements `Error` for — so `BeaconService::create_beacon`'s boxed
/// return type is accepted without a separate impl.
pub trait IntoApiErrorCause {
    /// Converts `self` into the stored cause.
    fn into_api_error_cause(self) -> ApiErrorCause;
}

/// Boxes a concrete error into the erased cause.
///
/// A free function rather than an expression inside the trait body: within
/// `impl<E: Error + Send + Sync + 'static> IntoApiErrorCause for E`,
/// `Arc::new(err) as Arc<dyn Error + Send + Sync>` cannot be proven `Send`/`Sync`,
/// because the compiler does not carry the impl-level bounds into the cast. Here
/// they are visible on the parameter.
fn erased_cause<E>(err: E) -> ApiErrorCause
where
    E: std::error::Error + Send + Sync + 'static,
{
    Arc::new(err)
}

impl<E> IntoApiErrorCause for E
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn into_api_error_cause(self) -> ApiErrorCause {
        erased_cause(self)
    }
}

#[derive(Debug)]
struct RetryAfterMsCause(u64);

impl std::fmt::Display for RetryAfterMsCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retry_after_ms={}", self.0)
    }
}

impl std::error::Error for RetryAfterMsCause {}

/// Structured API error: an HTTP semantic kind, a Matrix errcode, a user-facing
/// message, and an optional underlying cause for logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    /// HTTP semantic category that drives the response status code.
    pub kind: ApiErrorKind,
    /// Matrix-spec error code emitted in the response body's `errcode` field.
    pub code: MatrixErrorCode,
    /// User-facing error message; for `Internal` errors this is masked before
    /// being returned to the client (see [`ApiError::message`]).
    pub message: String,
    /// Optional opaque underlying error, surfaced through `std::error::Error::source`.
    #[serde(skip)]
    pub cause: Option<ApiErrorCause>,
}

// --- PartialEq: compare kind, code, message only (cause is opaque) ---

impl PartialEq for ApiError {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.code == other.code && self.message == other.message
    }
}

impl Eq for ApiError {}

// --- Display / Error ---

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_ref().map(|c| c.as_ref() as &(dyn std::error::Error + 'static))
    }
}

// ---------------------------------------------------------------------------
// Constructors (backward-compatible factory methods)
// ---------------------------------------------------------------------------

impl ApiError {
    // -- core constructors --

    /// Builds a 400 `M_BAD_JSON` error. Generic catch-all for malformed client input.
    /// Builds a 400 Bad Request error with `M_BAD_JSON` errcode.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::BadRequest, code: MatrixErrorCode::BadJson, message: message.into(), cause: None }
    }

    /// MSC4186: Construct an `M_UNKNOWN_POS` error — the sliding-sync `pos`
    /// token is invalid or expired. Clients branch on this errcode to reset
    /// their position and resync, distinguishing it from other 400 responses.
    pub fn unknown_pos(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::BadRequest, code: MatrixErrorCode::UnknownPos, message: message.into(), cause: None }
    }

    /// P-048 / B2 fix: Return HTTP 400 + M_BAD_PAGINATION when a pagination
    /// parameter (e.g. `since` token on `/sync`) is unparseable or invalid.
    /// Previously this used `ApiError::authentication()` → HTTP 401 M_UNKNOWN_TOKEN,
    /// which is wrong because the access token is valid — only the pagination
    /// cursor is bad. Matrix spec defines M_BAD_PAGINATION for "bad pagination
    /// query parameters" with HTTP 400 (Bad Request).
    pub fn bad_pagination(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::BadPagination,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC4133: the profile field **name** is longer than the spec's 255 bytes.
    ///
    /// 400 with `M_KEY_TOO_LARGE` (not `M_BAD_JSON`): the JSON is well-formed,
    /// the key is simply too long — clients branch on this to trim it.
    pub fn key_too_large(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::KeyTooLarge,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC4133: storing this value would push the profile past its 64 KiB limit.
    pub fn profile_too_large(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::ProfileTooLarge,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 401 error with `M_UNAUTHORIZED` Matrix code.
    /// Builds a 401 Unauthorized error with `M_UNAUTHORIZED` errcode.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::Unauthorized,
            message: message.into(),
            cause: None,
        }
    }

    /// Matrix spec (`client-server/login.yaml`) mandates HTTP 403 + M_FORBIDDEN
    /// when a login attempt fails (wrong password or unknown user) — 401 is
    /// reserved for missing/invalid access tokens, so using it here made generic
    /// 401 handling (e.g. token refresh) misfire on the login endpoint.
    ///
    /// UIA password checks share this constructor: Synapse's `LoginError` is
    /// likewise 403 + M_FORBIDDEN.
    pub fn invalid_credentials() -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::Forbidden,
            message: "Invalid credentials".to_string(),
            cause: None,
        }
    }

    /// Builds a 403 error with `M_FORBIDDEN` Matrix code.
    /// Builds a 403 Forbidden error with `M_FORBIDDEN` errcode.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Forbidden, code: MatrixErrorCode::Forbidden, message: message.into(), cause: None }
    }

    /// Builds a 404 error with `M_NOT_FOUND` Matrix code.
    /// Builds a 404 Not Found error with `M_NOT_FOUND` errcode.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::NotFound, code: MatrixErrorCode::NotFound, message: message.into(), cause: None }
    }

    /// Builds a 404 `M_UNRECOGNIZED` error for an unimplemented endpoint.
    ///
    /// Matrix defines no errcode for HTTP 501, so unimplemented endpoints are
    /// presented to clients as 404 `M_UNRECOGNIZED` (the spec-canonical status
    /// for "this endpoint/method is not recognized"). The distinction between
    /// "not implemented" and "unknown path" is preserved internally via
    /// [`ApiError::is_not_implemented`], which inspects the code.
    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotFound,
            code: MatrixErrorCode::Unimplemented,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a generic 409 Conflict error.
    ///
    /// Matrix defines no catch-all errcode for 409, so this carries `M_UNKNOWN`.
    /// Prefer a specific constructor — [`user_in_use`](Self::user_in_use) (400),
    /// [`room_in_use`](Self::room_in_use), [`threepid_in_use`](Self::threepid_in_use),
    /// or [`conflict_with`](Self::conflict_with) — whenever the conflict has a
    /// known Matrix errcode.
    ///
    /// Historically this hard-coded `M_USER_IN_USE`, which mislabelled every
    /// generic conflict (e.g. a duplicate tag reporting "user in use"). The
    /// intentional status/errcode divergence is registered in
    /// [`STATUS_DIVERGENCE_WHITELIST`].
    pub fn conflict(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Conflict, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }

    /// Conflict with a specific Matrix error code.
    pub fn conflict_with(code: MatrixErrorCode, message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Conflict, code, message: message.into(), cause: None }
    }

    /// Builds a 500 `M_UNKNOWN` internal error.
    /// Builds a 500 Internal error with `M_UNKNOWN` errcode.
    pub fn internal(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Internal, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }

    /// 503 Service Unavailable — required dependency (Redis, etc.) offline.
    /// Use this when the server cannot fulfill a request due to a downstream
    /// dependency being unavailable. The Matrix error code is M_UNKNOWN since
    /// 5xx responses typically don't carry a specific Matrix error code in the
    /// body, but having a stable code helps client retry logic.
    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::ServiceUnavailable,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC3806: Content scan failed — fail-closed.
    /// Returns 502 Bad Gateway + `M_CONTENT_SCAN_FAILED`.
    pub fn content_scan_failed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadGateway,
            code: MatrixErrorCode::ContentScanFailed,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC3806: Content scanner is disabled.
    /// Returns 501 Not Implemented + `M_CONTENT_SCAN_DISABLED`.
    pub fn content_scan_disabled(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotImplemented,
            code: MatrixErrorCode::ContentScanDisabled,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC4190: an application service called `POST /register` for a virtual user
    /// without `inhibit_login: true`.
    ///
    /// 返回 400 + `M_APPSERVICE_LOGIN_UNSUPPORTED`（与上游同码同状态）。语义是
    /// "服务器不会替 AS 登录"：AS 要么显式 `inhibit_login: true` 只建用户，要么改用
    /// `m.login.application_service` 走 `/login` 拿 token。
    pub fn appservice_login_unsupported(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::AppserviceLoginUnsupported,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC4512: the `fed_proxy` destination is this server itself, or a server
    /// that is broadly refused for federation.
    /// Returns 403 Forbidden + `M_FEDPROXY_DESTINATION_DENIED`.
    pub fn fed_proxy_destination_denied(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::FedProxyDestinationDenied,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC4512: the `fed_proxy` path is not under the calling appservice's
    /// `proxy_prefix`, or contains `.`/`..` segments.
    /// Returns 403 Forbidden + `M_FEDPROXY_PATH_NOT_ALLOWED`.
    pub fn fed_proxy_path_not_allowed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::FedProxyPathNotAllowed,
            message: message.into(),
            cause: None,
        }
    }

    /// MSC4512: the `fed_proxy` request could not be delivered to the
    /// destination server.
    /// Returns 502 Bad Gateway + `M_FEDPROXY_CONNECTION_FAILED`.
    pub fn fed_proxy_connection_failed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadGateway,
            code: MatrixErrorCode::FedProxyConnectionFailed,
            message: message.into(),
            cause: None,
        }
    }

    /// A well-formed request for a feature this server does not implement.
    /// Returns **501 Not Implemented** + `M_UNKNOWN`.
    ///
    /// Distinct from [`not_implemented`](Self::not_implemented), which is a 404
    /// `M_UNRECOGNIZED` for an endpoint that does not exist at all. Use this one
    /// when the endpoint exists and the request is understood, but a specific
    /// field or extension is unsupported: the alternative is to accept the field
    /// and silently discard it, which turns a client's explicit instruction into
    /// a no-op that looks like success.
    ///
    /// Matrix defines no errcode for 501, so the code is the catch-all
    /// `M_UNKNOWN` — registered as a status divergence for exactly that reason.
    pub fn not_supported(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::NotImplemented,
            code: MatrixErrorCode::Unknown,
            message: message.into(),
            cause: None,
        }
    }

    /// Log a database error and return an Internal error whose message carries the
    /// operation context only (the underlying DB error is logged, not exposed to the
    /// client). This is the canonical constructor for storage-layer failures where
    /// leaking SQL/table details to the client is undesirable.
    /// Builds a 500 Internal error for database failures, masking the DB detail from the client.
    pub fn database_with_context(context: &str, err: &dyn std::fmt::Display) -> Self {
        tracing::error!(%context, %err, "database error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Database error: {context}"),
            cause: None,
        }
    }

    /// Log an internal error and return an Internal error whose message carries the
    /// operation context only (mirrors `database_with_context` for non-DB failures).
    /// Builds a 500 Internal error wrapping a generic operation context for opaque failures.
    pub fn internal_with_context(context: &str, err: &dyn std::fmt::Display) -> Self {
        tracing::error!(%context, %err, "internal error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Internal error: {context}"),
            cause: None,
        }
    }

    /// Builds a 500 `M_UNKNOWN` error for a database failure.
    pub fn database(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Internal, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }

    /// Like [`database_with_context`](Self::database_with_context), but **keeps the
    /// underlying error** in [`cause`](Self::cause).
    ///
    /// `cause` is `#[serde(skip)]`, so it is never serialised into a response body —
    /// the client still only sees `Database error: <context>`. What changes is that
    /// logs and `std::error::Error::source()` can recover the real failure.
    ///
    /// Prefer this over `database_with_context` wherever a concrete error is in
    /// hand. The `&dyn Display` variant only logs, and in a test binary (no tracing
    /// subscriber installed) that log goes nowhere — leaving the unactionable
    /// `cause: None` that made a real DB failure undiagnosable.
    pub fn database_with_cause<E>(context: &str, err: E) -> Self
    where
        E: IntoApiErrorCause,
    {
        let cause = err.into_api_error_cause();
        tracing::error!(%context, error = %cause, "database error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Database error: {context}"),
            cause: Some(cause),
        }
    }

    /// Like [`internal_with_context`](Self::internal_with_context), but **keeps the
    /// underlying error** in [`cause`](Self::cause).
    ///
    /// See [`database_with_cause`](Self::database_with_cause) for the rationale and
    /// why it is safe to keep the detail (`cause` never reaches the client).
    pub fn internal_with_cause<E>(context: &str, err: E) -> Self
    where
        E: IntoApiErrorCause,
    {
        let cause = err.into_api_error_cause();
        tracing::error!(%context, error = %cause, "internal error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Internal error: {context}"),
            cause: Some(cause),
        }
    }

    /// Like [`internal_with_cause`](Self::internal_with_cause), for an
    /// **already-boxed** error.
    ///
    /// Exists because `Box<dyn Error + Send + Sync>` does not implement
    /// `std::error::Error` in Rust 1.93 (its blanket `impl<E: Error> Error for Box<E>`
    /// needs `E: Sized`, which a trait object is not). So the generic constructor
    /// cannot accept it, and this overload takes the box directly — no cast on a
    /// generic parameter, so the `Send`/`Sync` bounds hold.
    ///
    /// `BeaconService::create_beacon` returns exactly this shape.
    pub fn internal_with_boxed_cause(context: &str, err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        tracing::error!(%context, error = %err, "internal error");
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Internal error: {context}"),
            cause: Some(Arc::from(err)),
        }
    }

    /// Builds a 500 `M_UNKNOWN` error prefixed with `Cache error:` for a cache backend failure.
    pub fn cache(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Internal,
            code: MatrixErrorCode::Unknown,
            message: format!("Cache error: {}", message.into()),
            cause: None,
        }
    }

    /// Builds a 410 error using `M_NOT_FOUND` Matrix code.
    pub fn gone(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Gone, code: MatrixErrorCode::NotFound, message: message.into(), cause: None }
    }

    /// Builds a 401 error with `M_UNKNOWN_TOKEN` Matrix code.
    pub fn authentication(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::UnknownToken,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_INVALID_PARAM` error for failed input validation.
    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_INVALID_PARAM` error for malformed input.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 500 `M_UNKNOWN` error for a cryptographic failure.
    pub fn crypto(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Internal, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }

    /// Builds a 429 `M_LIMIT_EXCEEDED` error. Uses the caller-supplied message
    /// when non-empty, otherwise falls back to a generic one.
    pub fn rate_limited(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            kind: ApiErrorKind::RateLimited,
            code: MatrixErrorCode::LimitExceeded,
            message: if message.is_empty() { "Rate limited".to_string() } else { message },
            cause: None,
        }
    }

    /// Builds a 429 error carrying a specific `retry_after_ms` hint.
    pub fn rate_limited_with_retry(retry_after_ms: u64) -> Self {
        Self {
            kind: ApiErrorKind::RateLimited,
            code: MatrixErrorCode::LimitExceeded,
            message: "Rate limited".to_string(),
            cause: Some(Arc::new(RetryAfterMsCause(retry_after_ms))),
        }
    }

    /// Builds a 401 `M_MISSING_TOKEN` error for absent access tokens.
    pub fn missing_token() -> Self {
        Self {
            kind: ApiErrorKind::Unauthorized,
            code: MatrixErrorCode::MissingToken,
            message: "Missing access token".to_string(),
            cause: None,
        }
    }

    /// Builds a 400 error with `M_NOT_JSON` when JSON parsing fails.
    pub fn not_json(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::BadRequest, code: MatrixErrorCode::NotJson, message: message.into(), cause: None }
    }

    // -- domain-specific constructors (delegate to core with specific code) --

    /// Builds a 403 `M_USER_DEACTIVATED` error.
    pub fn user_deactivated(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::UserDeactivated,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_INVALID_USERNAME` error.
    pub fn invalid_username(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidUsername,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_USER_IN_USE` error for duplicate user IDs.
    pub fn user_in_use(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::BadRequest, code: MatrixErrorCode::UserInUse, message: message.into(), cause: None }
    }

    /// Builds a 409 `M_ROOM_IN_USE` error.
    pub fn room_in_use(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Conflict, code: MatrixErrorCode::RoomInUse, message: message.into(), cause: None }
    }

    /// Builds a 400 `M_INVALID_ROOM_STATE` error.
    pub fn invalid_room_state(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidRoomState,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 409 `M_THREEPID_IN_USE` error for duplicate third-party IDs.
    pub fn threepid_in_use(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Conflict,
            code: MatrixErrorCode::ThreepidInUse,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_THREEPID_NOT_FOUND` error.
    pub fn threepid_not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::ThreepidNotFound,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 403 `M_THREEPID_AUTH_FAILED` error.
    pub fn threepid_auth_failed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ThreepidAuthFailed,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 403 `M_THREEPID_DENIED` error.
    pub fn threepid_denied(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ThreepidDenied,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 403 `M_SERVER_NOT_TRUSTED` error.
    pub fn server_not_trusted(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ServerNotTrusted,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_UNSUPPORTED_ROOM_VERSION` error.
    pub fn unsupported_room_version(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::UnsupportedRoomVersion,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_INCOMPATIBLE_ROOM_VERSION` error.
    pub fn incompatible_room_version(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::IncompatibleRoomVersion,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_BAD_STATE` error.
    pub fn bad_state(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::BadRequest, code: MatrixErrorCode::BadState, message: message.into(), cause: None }
    }

    /// Builds a 403 `M_GUEST_ACCESS_FORBIDDEN` error.
    pub fn guest_access_forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::GuestAccessForbidden,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_CAPTCHA_NEEDED` error.
    pub fn captcha_needed(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::CaptchaNeeded,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_CAPTCHA_INVALID` error.
    pub fn captcha_invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::CaptchaInvalid,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_MISSING_PARAM` error.
    pub fn missing_param(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::MissingParam,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 400 `M_INVALID_PARAM` error.
    pub fn invalid_param(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest,
            code: MatrixErrorCode::InvalidParam,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 413 `M_TOO_LARGE` error.
    pub fn too_large(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::PayloadTooLarge,
            code: MatrixErrorCode::TooLarge,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 409 `M_EXCLUSIVE` error.
    pub fn exclusive(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Conflict, code: MatrixErrorCode::Exclusive, message: message.into(), cause: None }
    }

    /// Builds a 403 `M_RESOURCE_LIMIT_EXCEEDED` error.
    pub fn resource_limit_exceeded(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::ResourceLimitExceeded,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 403 `M_CANNOT_LEAVE_SERVER_NOTICE_ROOM` error.
    pub fn cannot_leave_server_notice_room(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Forbidden,
            code: MatrixErrorCode::CannotLeaveServerNoticeRoom,
            message: message.into(),
            cause: None,
        }
    }

    /// Builds a 500 `M_UNKNOWN` error.
    pub fn unknown(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Internal, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }

    /// Builds a 404 `M_UNRECOGNIZED` error for unrecognized request payloads.
    pub fn unrecognized(message: impl Into<String>) -> Self {
        // M_UNRECOGNIZED maps to HTTP 404 per Matrix spec (unknown endpoint).
        // The router-level fallback for truly unknown paths is hardcoded in
        // assembly.rs; this constructor is used for otherwise-unrecognized
        // requests that reach a handler.
        Self { kind: ApiErrorKind::NotFound, code: MatrixErrorCode::Unrecognized, message: message.into(), cause: None }
    }

    /// Builds a 504 `M_REQUEST_TIMEOUT` error for upstream timeout.
    pub fn request_timeout(message: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Timeout,
            code: MatrixErrorCode::RequestTimeout,
            message: message.into(),
            cause: None,
        }
    }

    // -- encryption / decryption (map to Internal with specific message) --

    /// Builds a 500 Internal error for decryption failures.
    pub fn decryption_error(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Internal, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }

    /// Builds a 500 `M_UNKNOWN` error for encryption failure.
    pub fn encryption_error(message: impl Into<String>) -> Self {
        Self { kind: ApiErrorKind::Internal, code: MatrixErrorCode::Unknown, message: message.into(), cause: None }
    }
}

// ---------------------------------------------------------------------------
// Builder / convenience methods
// ---------------------------------------------------------------------------

impl ApiError {
    /// Override the Matrix error code (use sparingly).
    pub fn with_code(mut self, code: MatrixErrorCode) -> Self {
        self.code = code;
        self
    }

    // -- kind predicates (replace match / matches!) --

    /// Returns `true` if `kind == BadRequest`.
    pub fn is_bad_request(&self) -> bool {
        self.kind == ApiErrorKind::BadRequest
    }
    /// Returns `true` if `kind == Unauthorized`.
    pub fn is_unauthorized(&self) -> bool {
        self.kind == ApiErrorKind::Unauthorized
    }
    /// Returns `true` if `kind == Forbidden`.
    pub fn is_forbidden(&self) -> bool {
        self.kind == ApiErrorKind::Forbidden
    }
    /// Returns `true` if `kind == NotFound`.
    pub fn is_not_found(&self) -> bool {
        self.kind == ApiErrorKind::NotFound
    }
    /// Returns `true` if `kind == Conflict`.
    pub fn is_conflict(&self) -> bool {
        self.kind == ApiErrorKind::Conflict
    }
    /// Returns `true` if `kind == Gone`.
    pub fn is_gone(&self) -> bool {
        self.kind == ApiErrorKind::Gone
    }
    /// Returns `true` if `kind == RateLimited`.
    pub fn is_rate_limited(&self) -> bool {
        self.kind == ApiErrorKind::RateLimited
    }
    /// Returns `true` if `kind == Internal`.
    pub fn is_internal(&self) -> bool {
        self.kind == ApiErrorKind::Internal
    }
    /// Returns `true` if this error represents an unimplemented operation.
    ///
    /// Code-aware: the `Unimplemented` errcode shares the wire string
    /// `M_UNRECOGNIZED` with the (unrelated) `Unrecognized` errcode, so the
    /// distinction lives in the code, not the HTTP status.
    pub fn is_not_implemented(&self) -> bool {
        self.kind == ApiErrorKind::NotImplemented || self.code == MatrixErrorCode::Unimplemented
    }
    /// Returns `true` if `kind == Timeout`.
    pub fn is_timeout(&self) -> bool {
        self.kind == ApiErrorKind::Timeout
    }

    // -- code predicates (for specific Matrix error code checks) --

    /// Returns `true` if the Matrix code equals `code`.
    pub fn code_is(&self, code: MatrixErrorCode) -> bool {
        self.code == code
    }

    // -- accessors --

    /// Returns a reference to the Matrix error code.
    pub fn code(&self) -> &MatrixErrorCode {
        &self.code
    }

    /// Returns the Matrix error code by value.
    pub fn matrix_code(&self) -> MatrixErrorCode {
        self.code
    }

    /// Returns the Matrix error code as a static string (`"M_FORBIDDEN"`, etc.).
    pub fn code_str(&self) -> &'static str {
        self.code.as_str()
    }

    /// Returns the user-facing error message.
    pub fn message(&self) -> String {
        match self.kind {
            ApiErrorKind::Internal => {
                tracing::error!(
                    kind = ?self.kind,
                    code = %self.code,
                    message = %self.message,
                    "Internal error returned to client"
                );
                "An internal error occurred".to_string()
            }
            _ => self.message.clone(),
        }
    }

    /// Returns the full internal message (for logging, never shown to clients).
    pub fn internal_message(&self) -> &str {
        &self.message
    }

    /// Returns the canonical HTTP status for this error.
    pub fn http_status(&self) -> StatusCode {
        self.kind.default_http_status()
    }

    /// Returns the retry-after hint in milliseconds for rate-limited errors,
    /// or `None` for non-rate-limited errors.
    pub fn retry_after_ms(&self) -> Option<u64> {
        if self.kind == ApiErrorKind::RateLimited {
            self.cause
                .as_ref()
                .and_then(|cause| cause.downcast_ref::<RetryAfterMsCause>().map(|retry| retry.0))
                .or(Some(5000))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// IntoResponse (Axum integration)
// ---------------------------------------------------------------------------

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.kind == ApiErrorKind::Gone {
            let status_code = StatusCode::GONE;
            return (
                status_code,
                Json(
                    serde_json::from_str::<serde_json::Value>(&self.message)
                        .unwrap_or_else(|_| json!({"errcode": self.code_str(), "error": self.message})),
                ),
            )
                .into_response();
        }

        let errcode = self.code_str().to_string();
        let error_msg = self.message();
        // Use `kind` (explicit HTTP semantic kind) for the HTTP status code,
        // and `code` (Matrix error code) only for the `errcode` JSON field.
        // The two are set independently by each constructor, so an errcode is
        // not hard-wired to a single HTTP status.
        let status_code = self.kind.default_http_status();

        // Emit metrics
        if let Some(collector) = ERROR_METRICS.get() {
            let metric_name = format!("http_errors_total_{}", errcode);
            if let Some(counter) = collector.get_counter(&metric_name) {
                counter.inc();
            } else {
                let mut labels = std::collections::HashMap::new();
                labels.insert("errcode".to_string(), errcode.clone());
                let counter = collector.register_counter_with_labels(metric_name, labels);
                counter.inc();
            }
        }

        let retry_after_ms = self.retry_after_ms();

        let mut body = json!({
            "errcode": errcode,
            "error": error_msg
        });

        if let Some(ms) = retry_after_ms {
            body["retry_after_ms"] = json!(ms);
        }

        let mut response = (status_code, Json(body)).into_response();

        // S-9-A1: Standardize rate limit headers on 429 responses
        // - Retry-After: Standard RFC 7231 header with seconds (rounded up)
        // - X-RateLimit-Retry-After-Ms: Millisecond precision for Matrix clients
        // - X-RateLimit-Limit: Current rate limit threshold (from config or default)
        // - X-RateLimit-Remaining: Remaining requests in current window (always 0 on 429)
        if let Some(ms) = retry_after_ms {
            let retry_after_seconds = ms.saturating_add(999) / 1000;
            if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
                response.headers_mut().insert("retry-after", value);
            }
            if let Ok(value) = HeaderValue::from_str(&ms.to_string()) {
                response.headers_mut().insert("x-ratelimit-retry-after-ms", value);
            }
            // Always set remaining to 0 on 429
            response.headers_mut().insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        }
        response
    }
}

// ---------------------------------------------------------------------------
// From impls (error conversion)
// ---------------------------------------------------------------------------

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        // Classify via the structured database error kind (SQLSTATE) instead of
        // fragile string matching on the display text. `is_unique_violation()`
        // is populated by the Postgres driver from the 23505 unique_violation
        // code, so localized/custom messages can't cause false negatives or
        // positives (审查 #19).
        let is_unique_violation = err.as_database_error().map(|e| e.is_unique_violation()).unwrap_or(false);

        // Metrics: this `From` impl is the single point every storage-layer
        // `sqlx::Error` funnels through on its way to an `ApiError`, so it is the
        // natural place to count DB failures. `db_query_duration_ms` is fed
        // separately by `DbQueryMetricsLayer` (sqlx reports duration but no
        // success flag — see `db_query_metrics.rs`).
        //
        // Two "expected" variants are excluded on purpose:
        //   * `RowNotFound` — a normal "no rows matched" outcome. Counting it
        //     would make `db_query_errors` track empty fetches, not failures.
        //   * unique violations — business-level conflicts, classified as 400 below.
        if !is_unique_violation && !matches!(&err, sqlx::Error::RowNotFound) {
            if let Some(metrics) = crate::server_metrics::global_server_metrics() {
                metrics.db_query_errors.inc();
            }
        }

        if is_unique_violation {
            tracing::error!(%err, "duplicate database entry");
            ApiError::bad_request("A duplicate entry was found")
        } else {
            tracing::error!(%err, "database error");
            ApiError::database("A database error occurred")
        }
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(err: redis::RedisError) -> Self {
        tracing::error!(%err, "redis error");
        ApiError::cache(err.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::not_json(err.to_string())
    }
}

impl From<std::string::FromUtf8Error> for ApiError {
    fn from(_err: std::string::FromUtf8Error) -> Self {
        ApiError::validation("Invalid UTF-8 encoding")
    }
}

impl From<std::num::ParseIntError> for ApiError {
    fn from(_err: std::num::ParseIntError) -> Self {
        ApiError::validation("Invalid number format")
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::internal(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// ApiResponse (unchanged)
// ---------------------------------------------------------------------------

/// Uniform envelope for HTTP handler return values: a `status` string plus
/// either `data` (success) or `error` + `errcode` (failure).
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Always `"ok"` for successes or `"error"` for failures.
    pub status: String,
    /// Success payload; `None` on error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error message; `None` on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Matrix error code; `None` on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errcode: Option<String>,
    /// Retry-after hint in milliseconds; `None` unless rate-limited.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl<T> ApiResponse<T> {
    /// Wraps `data` in a successful `ApiResponse`.
    pub fn success(data: T) -> Self {
        Self { status: "ok".to_string(), data: Some(data), error: None, errcode: None, retry_after_ms: None }
    }

    /// Wraps an error message and Matrix errcode in an `ApiResponse`.
    pub fn error(error: String, errcode: String) -> Self {
        Self {
            status: "error".to_string(),
            data: None,
            error: Some(error),
            errcode: Some(errcode),
            retry_after_ms: None,
        }
    }

    /// Wraps an error message and Matrix errcode with a `retry_after_ms` hint.
    pub fn error_with_retry(error: String, errcode: String, retry_after_ms: u64) -> Self {
        Self {
            status: "error".to_string(),
            data: None,
            error: Some(error),
            errcode: Some(errcode),
            retry_after_ms: Some(retry_after_ms),
        }
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let status_code = if self.status == "ok" {
            StatusCode::OK
        } else {
            // 收敛到 MatrixErrorCode::from_str + http_status() 单一映射源，
            // 消除第三套硬编码 errcode→状态码映射的漂移（未知 errcode 落 500）。
            self.errcode
                .as_deref()
                .and_then(MatrixErrorCode::from_str)
                .map(|code| code.http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
        };
        (status_code, Json(self)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Type alias
// ---------------------------------------------------------------------------

/// Standard `Result` alias for handler return types: `Result<T, ApiError>`.
pub type ApiResult<T> = Result<T, ApiError>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
