//! Project-wide utility macros (`map_internal!`, `map_database!`).

#[macro_export]
/// Maps an `Err` to an Internal `ApiError` with a context message.
///
/// The message stays masked (context only), but the **underlying error is preserved
/// in `ApiError::cause`**, which is `#[serde(skip)]` and therefore never reaches a
/// client response. Dropping it made real failures undiagnosable: with no tracing
/// subscriber installed (every test binary) the log line went nowhere, so all that
/// survived was `Internal error: <context>, cause: None`.
macro_rules! map_internal {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::internal_with_cause($msg, e))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::internal_with_cause($msg, e))
    };
    ($msg:literal) => {
        |e| $crate::ApiError::internal_with_cause($msg, e)
    };
}

/// Map a storage-layer error to an Internal error whose message carries only the
/// operation context (the underlying error is logged, not exposed to the client).
/// Prefer this over `map_internal!` for DB failures.
#[macro_export]
macro_rules! map_database {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::database_with_cause($msg, e))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::database_with_cause($msg, e))
    };
    ($msg:literal) => {
        |e| $crate::ApiError::database_with_cause($msg, e)
    };
}
