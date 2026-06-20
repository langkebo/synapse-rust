use std::sync::Arc;

#[derive(Clone)]
pub struct OidcUserMappingStorage {
    pool: Arc<sqlx::PgPool>,
}

impl OidcUserMappingStorage {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    pub async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT user_id FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2")
            .bind(issuer)
            .bind(subject)
            .fetch_optional(&*self.pool)
            .await
    }

    pub async fn update_last_authenticated(&self, issuer: &str, subject: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE oidc_user_mapping SET last_authenticated_ts = $1, \
             authentication_count = authentication_count + 1 \
             WHERE issuer = $2 AND subject = $3",
        )
        .bind(now_ts)
        .bind(issuer)
        .bind(subject)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    pub async fn insert_mapping(
        &self,
        issuer: &str,
        subject: &str,
        user_id: &str,
        now_ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO oidc_user_mapping \
             (issuer, subject, user_id, first_seen_ts, last_authenticated_ts, authentication_count) \
             VALUES ($1, $2, $3, $4, $4, 1)",
        )
        .bind(issuer)
        .bind(subject)
        .bind(user_id)
        .bind(now_ts)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
}
