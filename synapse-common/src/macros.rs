#[macro_export]
macro_rules! impl_api_error {
    ($enum:ident, $($variant:ident => ($code:expr, $status:expr)),+) => {
        $(
            pub fn $variant(message: String) -> Self {
                Self::$variant {
                    code: $code.to_string(),
                    message,
                    status: $status,
                }
            }
        )+
    };
}

#[macro_export]
macro_rules! map_internal {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::internal_with_log($msg, &e))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::internal_with_log($msg, &e))
    };
    ($msg:literal) => {
        |e| $crate::ApiError::internal_with_log($msg, &e)
    };
}

/// Map a storage-layer error to an Internal error whose message carries only the
/// operation context (the underlying error is logged, not exposed to the client).
/// Prefer this over `map_internal!` for DB failures.
#[macro_export]
macro_rules! map_database {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::database_with_context($msg, &e))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::database_with_context($msg, &e))
    };
    ($msg:literal) => {
        |e| $crate::ApiError::database_with_context($msg, &e)
    };
}

#[macro_export]
macro_rules! map_bad_request {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::bad_request(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::bad_request(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_not_found {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::not_found(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::not_found(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_unauthorized {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::unauthorized(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::unauthorized(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_forbidden {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::ApiError::forbidden(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::ApiError::forbidden(format!("{}: {}", $msg, e)))
    };
}
