use async_trait::async_trait;
use std::sync::Arc;

// ── Trait ───────────────────────────────────────────────────────────────

/// Storage-agnostic API for OIDC user mapping persistence.
///
/// Implemented by [`OidcUserMappingStorage`] (Postgres) and
/// [`crate::test_mocks::InMemoryOidcUserMappingStore`] (in-memory).
#[async_trait]
pub trait OidcUserMappingStoreApi: Send + Sync {
    /// See [`get_bound_user_id`].
    async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error>;
    /// See [`update_last_authenticated`].
    async fn update_last_authenticated(&self, issuer: &str, subject: &str, now_ts: i64) -> Result<(), sqlx::Error>;
    /// See [`insert_mapping`].
    async fn insert_mapping(&self, issuer: &str, subject: &str, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error>;
}

// ── Postgres implementation ─────────────────────────────────────────────

/// The `OidcUserMappingStorage` struct.
#[derive(Clone)]
pub struct OidcUserMappingStorage {
    pool: Arc<sqlx::PgPool>,
}

impl OidcUserMappingStorage {
    /// See [`new`].
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }

    /// See [`get_bound_user_id`].
    pub async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT user_id FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2")
            .bind(issuer)
            .bind(subject)
            .fetch_optional(&*self.pool)
            .await
    }

    /// See [`update_last_authenticated`].
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

    /// See [`insert_mapping`].
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

// ── Trait delegation ────────────────────────────────────────────────────

#[async_trait]
impl OidcUserMappingStoreApi for OidcUserMappingStorage {
    async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_bound_user_id(issuer, subject).await
    }

    async fn update_last_authenticated(&self, issuer: &str, subject: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        self.update_last_authenticated(issuer, subject, now_ts).await
    }

    async fn insert_mapping(&self, issuer: &str, subject: &str, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        self.insert_mapping(issuer, subject, user_id, now_ts).await
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod db_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use std::sync::Arc;
    use std::time::Duration;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn make_suffix() -> String {
        uuid::Uuid::new_v4().simple().to_string()
    }

    #[tokio::test]
    async fn get_bound_user_id_none_for_missing_mapping() {
        let pool = test_pool().await;
        let storage = OidcUserMappingStorage::new(pool);
        let suffix = make_suffix();
        let issuer = format!("https://issuer_{suffix}.example.com");
        let subject = format!("subject_{suffix}");
        assert!(storage.get_bound_user_id(&issuer, &subject).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn insert_mapping_then_get_bound_user_id() {
        let pool = test_pool().await;
        let storage = OidcUserMappingStorage::new(pool.clone());
        let suffix = make_suffix();
        let issuer = format!("https://issuer_{suffix}.example.com");
        let subject = format!("subject_{suffix}");
        let user_id = format!("@oidc_user_{suffix}:test");

        storage.insert_mapping(&issuer, &subject, &user_id, 1000).await.unwrap();
        assert_eq!(storage.get_bound_user_id(&issuer, &subject).await.unwrap().as_deref(), Some(user_id.as_str()));

        let _ = sqlx::query("DELETE FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2")
            .bind(&issuer)
            .bind(&subject)
            .execute(pool.as_ref())
            .await;
    }

    #[tokio::test]
    async fn update_last_authenticated_increments_count() {
        let pool = test_pool().await;
        let storage = OidcUserMappingStorage::new(pool.clone());
        let suffix = make_suffix();
        let issuer = format!("https://issuer_{suffix}.example.com");
        let subject = format!("subject_{suffix}");
        let user_id = format!("@oidc_update_{suffix}:test");

        storage.insert_mapping(&issuer, &subject, &user_id, 1000).await.unwrap();
        storage.update_last_authenticated(&issuer, &subject, 2000).await.unwrap();

        let (ts, count): (i64, i32) =
            sqlx::query_as("SELECT last_authenticated_ts, authentication_count FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2")
                .bind(&issuer)
                .bind(&subject)
                .fetch_one(pool.as_ref())
                .await
                .unwrap();
        assert_eq!(ts, 2000);
        assert_eq!(count, 2);

        let _ = sqlx::query("DELETE FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2")
            .bind(&issuer)
            .bind(&subject)
            .execute(pool.as_ref())
            .await;
    }

    #[tokio::test]
    async fn insert_mapping_duplicate_issuer_subject_errors() {
        let pool = test_pool().await;
        let storage = OidcUserMappingStorage::new(pool.clone());
        let suffix = make_suffix();
        let issuer = format!("https://issuer_{suffix}.example.com");
        let subject = format!("subject_{suffix}");
        let user_id = format!("@oidc_dup_{suffix}:test");

        storage.insert_mapping(&issuer, &subject, &user_id, 1000).await.unwrap();
        // 唯一约束 (issuer, subject)，重复插入应报错
        assert!(storage.insert_mapping(&issuer, &subject, &user_id, 2000).await.is_err());

        let _ = sqlx::query("DELETE FROM oidc_user_mapping WHERE issuer = $1 AND subject = $2")
            .bind(&issuer)
            .bind(&subject)
            .execute(pool.as_ref())
            .await;
    }
}
