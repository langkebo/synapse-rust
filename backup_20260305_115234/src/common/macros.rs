#[macro_export]
macro_rules! impl_sqlx_types {
    ($type:ty) => {
        impl $type {
            pub async fn from_db(pool: &sqlx::PgPool) -> Result<Option<Self>, sqlx::Error> {
                sqlx::query_as::<_, Self>("SELECT * FROM users WHERE active = true")
                    .fetch_optional(pool)
                    .await
            }
        }
    };
}

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
        $result.map_err(|e| $crate::common::ApiError::internal(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::common::ApiError::internal(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_bad_request {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::common::ApiError::bad_request(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::common::ApiError::bad_request(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_not_found {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::common::ApiError::not_found(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::common::ApiError::not_found(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_unauthorized {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::common::ApiError::unauthorized(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::common::ApiError::unauthorized(format!("{}: {}", $msg, e)))
    };
}

#[macro_export]
macro_rules! map_forbidden {
    ($result:expr, $msg:literal) => {
        $result.map_err(|e| $crate::common::ApiError::forbidden(format!("{}: {}", $msg, e)))
    };
    ($result:expr, $msg:expr) => {
        $result.map_err(|e| $crate::common::ApiError::forbidden(format!("{}: {}", $msg, e)))
    };
}
