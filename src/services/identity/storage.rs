use super::models::ThirdPartyId;
use sqlx::PgPool;

#[derive(sqlx::FromRow)]
struct ThreePidRow {
    address: Option<String>,
    medium: Option<String>,
    user_id: Option<String>,
    validated_at: Option<i64>,
    added_at: Option<i64>,
}

#[derive(Clone)]
pub struct IdentityStorage {
    pool: PgPool,
}

impl IdentityStorage {
    pub fn new(pool: &PgPool) -> Self {
        Self {
            pool: pool.clone(),
        }
    }

    pub async fn get_user_three_pids(&self, user_id: &str) -> Result<Vec<ThirdPartyId>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ThreePidRow>(
            r#"
            SELECT address, medium, user_id, validated_at, added_at
            FROM user_threepids
            WHERE user_id = $1
            ORDER BY added_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|r| {
                Some(ThirdPartyId {
                    address: r.address?,
                    medium: r.medium?,
                    user_id: r.user_id?,
                    validated_at: r.validated_at?,
                    added_at: r.added_at?,
                })
            })
            .collect())
    }

    pub async fn add_three_pid(&self, three_pid: &ThirdPartyId) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO user_threepids (address, medium, user_id, validated_at, added_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (address, medium, user_id) DO UPDATE SET
                validated_at = EXCLUDED.validated_at
            "#,
            three_pid.address,
            three_pid.medium,
            three_pid.user_id,
            three_pid.validated_at,
            three_pid.added_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn remove_three_pid(&self, address: &str, medium: &str, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM user_threepids
            WHERE address = $1 AND medium = $2 AND user_id = $3
            "#,
            address,
            medium,
            user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_three_pid_user(&self, address: &str, medium: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT user_id FROM user_threepids
            WHERE address = $1 AND medium = $2
            "#,
            address,
            medium
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.user_id))
    }

    pub async fn validate_three_pid(&self, address: &str, medium: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r#"
            UPDATE user_threepids
            SET validated_at = $4
            WHERE address = $1 AND medium = $2 AND user_id = $3
            "#,
            address,
            medium,
            user_id,
            now
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_pending_three_pid_validations(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT address, medium, user_id, validated_at, added_at
            FROM user_threepids
            WHERE validated_at < added_at
            ORDER BY added_at DESC
            LIMIT 100
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "address": r.address,
                    "medium": r.medium,
                    "user_id": r.user_id,
                    "validated_at": r.validated_at,
                    "added_at": r.added_at
                })
            })
            .collect())
    }
}
