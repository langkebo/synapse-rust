#[macro_export]
macro_rules! impl_sqlx_types {
    ($type:ty) => {
        impl $type {
            pub async fn from_db(pool: &sqlx::PgPool) -> Result<Option<Self>, sqlx::Error> {
                sqlx::query_as!(Self, "SELECT * FROM users WHERE active = true")
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
