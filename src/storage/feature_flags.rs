use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeatureFlagRecord {
    pub flag_key: String,
    pub target_scope: String,
    pub rollout_percent: i32,
    pub expires_at: Option<i64>,
    pub reason: String,
    pub status: String,
    pub created_by: String,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeatureFlagTargetRecord {
    pub id: i64,
    pub flag_key: String,
    pub subject_type: String,
    pub subject_id: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    pub flag_key: String,
    pub target_scope: String,
    pub rollout_percent: i32,
    pub expires_at: Option<i64>,
    pub reason: String,
    pub status: String,
    pub created_by: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub targets: Vec<FeatureFlagTargetRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagTargetInput {
    pub subject_type: String,
    pub subject_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFeatureFlagRequest {
    pub flag_key: String,
    pub target_scope: String,
    pub rollout_percent: i32,
    pub expires_at: Option<i64>,
    pub reason: String,
    pub status: Option<String>,
    #[serde(default)]
    pub targets: Vec<FeatureFlagTargetInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateFeatureFlagRequest {
    pub rollout_percent: Option<i32>,
    pub expires_at: Option<i64>,
    pub reason: Option<String>,
    pub status: Option<String>,
    pub targets: Option<Vec<FeatureFlagTargetInput>>,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureFlagFilters {
    pub target_scope: Option<String>,
    pub status: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Clone)]
pub struct FeatureFlagStorage {
    pool: Arc<PgPool>,
}

impl FeatureFlagStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create_flag(
        &self,
        request: &CreateFeatureFlagRequest,
        created_by: &str,
        created_ts: i64,
    ) -> Result<FeatureFlag, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        let record = sqlx::query_as::<_, FeatureFlagRecord>(
            r#"
            INSERT INTO feature_flags (
                flag_key,
                target_scope,
                rollout_percent,
                expires_at,
                reason,
                status,
                created_by,
                created_ts,
                updated_ts
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
            RETURNING flag_key, target_scope, rollout_percent, expires_at, reason, status, created_by, created_ts, updated_ts
            "#,
        )
        .bind(&request.flag_key)
        .bind(&request.target_scope)
        .bind(request.rollout_percent)
        .bind(request.expires_at)
        .bind(&request.reason)
        .bind(request.status.as_deref().unwrap_or("draft"))
        .bind(created_by)
        .bind(created_ts)
        .fetch_one(&mut *transaction)
        .await?;

        let targets = self
            .replace_targets(
                &mut transaction,
                &request.flag_key,
                created_ts,
                &request.targets,
            )
            .await?;

        transaction.commit().await?;

        Ok(to_feature_flag(record, targets))
    }

    pub async fn update_flag(
        &self,
        flag_key: &str,
        request: &UpdateFeatureFlagRequest,
        updated_ts: i64,
    ) -> Result<Option<FeatureFlag>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        let record = sqlx::query_as::<_, FeatureFlagRecord>(
            r#"
            UPDATE feature_flags
            SET rollout_percent = COALESCE($2, rollout_percent),
                expires_at = COALESCE($3, expires_at),
                reason = COALESCE($4, reason),
                status = COALESCE($5, status),
                updated_ts = $6
            WHERE flag_key = $1
            RETURNING flag_key, target_scope, rollout_percent, expires_at, reason, status, created_by, created_ts, updated_ts
            "#,
        )
        .bind(flag_key)
        .bind(request.rollout_percent)
        .bind(request.expires_at)
        .bind(request.reason.as_deref())
        .bind(request.status.as_deref())
        .bind(updated_ts)
        .fetch_optional(&mut *transaction)
        .await?;

        let Some(record) = record else {
            transaction.rollback().await?;
            return Ok(None);
        };

        let targets = match &request.targets {
            Some(targets) => {
                self.replace_targets(&mut transaction, flag_key, updated_ts, targets)
                    .await?
            }
            None => Vec::new(),
        };

        transaction.commit().await?;

        let targets = if request.targets.is_some() {
            targets
        } else {
            let mut targets_by_flag = self.list_targets(&[flag_key.to_string()]).await?;
            targets_by_flag.remove(flag_key).unwrap_or_default()
        };

        Ok(Some(to_feature_flag(record, targets)))
    }

    pub async fn get_flag(&self, flag_key: &str) -> Result<Option<FeatureFlag>, sqlx::Error> {
        let record = sqlx::query_as::<_, FeatureFlagRecord>(
            r#"
            SELECT flag_key, target_scope, rollout_percent, expires_at, reason, status, created_by, created_ts, updated_ts
            FROM feature_flags
            WHERE flag_key = $1
            "#,
        )
        .bind(flag_key)
        .fetch_optional(&*self.pool)
        .await?;

        let Some(record) = record else {
            return Ok(None);
        };

        let targets = self.list_targets(&[flag_key.to_string()]).await?;
        Ok(Some(to_feature_flag(
            record,
            targets.get(flag_key).cloned().unwrap_or_default(),
        )))
    }

    pub async fn list_flags(
        &self,
        filters: &FeatureFlagFilters,
    ) -> Result<(Vec<FeatureFlag>, i64), sqlx::Error> {
        let mut count_query =
            QueryBuilder::<Postgres>::new("SELECT COUNT(*)::BIGINT FROM feature_flags WHERE 1=1");
        if let Some(ref target_scope) = filters.target_scope {
            count_query.push(" AND target_scope = ");
            count_query.push_bind(target_scope);
        }
        if let Some(ref status) = filters.status {
            count_query.push(" AND status = ");
            count_query.push_bind(status);
        }
        let total = count_query
            .build_query_scalar::<i64>()
            .fetch_one(&*self.pool)
            .await?;

        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT flag_key, target_scope, rollout_percent, expires_at, reason, status, created_by, created_ts, updated_ts FROM feature_flags WHERE 1=1",
        );
        if let Some(ref target_scope) = filters.target_scope {
            query.push(" AND target_scope = ");
            query.push_bind(target_scope);
        }
        if let Some(ref status) = filters.status {
            query.push(" AND status = ");
            query.push_bind(status);
        }
        query.push(" ORDER BY updated_ts DESC, flag_key ASC LIMIT ");
        query.push_bind(filters.limit);
        query.push(" OFFSET ");
        query.push_bind(filters.offset);

        let records = query
            .build_query_as::<FeatureFlagRecord>()
            .fetch_all(&*self.pool)
            .await?;
        let keys: Vec<String> = records
            .iter()
            .map(|record| record.flag_key.clone())
            .collect();
        let targets = self.list_targets(&keys).await?;

        let flags = records
            .into_iter()
            .map(|record| {
                let flag_key = record.flag_key.clone();
                to_feature_flag(record, targets.get(&flag_key).cloned().unwrap_or_default())
            })
            .collect();

        Ok((flags, total))
    }

    async fn replace_targets(
        &self,
        transaction: &mut sqlx::Transaction<'_, Postgres>,
        flag_key: &str,
        created_ts: i64,
        targets: &[FeatureFlagTargetInput],
    ) -> Result<Vec<FeatureFlagTargetRecord>, sqlx::Error> {
        sqlx::query("DELETE FROM feature_flag_targets WHERE flag_key = $1")
            .bind(flag_key)
            .execute(&mut **transaction)
            .await?;

        let mut inserted = Vec::with_capacity(targets.len());
        for target in targets {
            let record = sqlx::query_as::<_, FeatureFlagTargetRecord>(
                r#"
                INSERT INTO feature_flag_targets (
                    flag_key,
                    subject_type,
                    subject_id,
                    created_ts
                )
                VALUES ($1, $2, $3, $4)
                RETURNING id, flag_key, subject_type, subject_id, created_ts
                "#,
            )
            .bind(flag_key)
            .bind(&target.subject_type)
            .bind(&target.subject_id)
            .bind(created_ts)
            .fetch_one(&mut **transaction)
            .await?;
            inserted.push(record);
        }

        Ok(inserted)
    }

    async fn list_targets(
        &self,
        flag_keys: &[String],
    ) -> Result<HashMap<String, Vec<FeatureFlagTargetRecord>>, sqlx::Error> {
        if flag_keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT id, flag_key, subject_type, subject_id, created_ts FROM feature_flag_targets WHERE flag_key IN (",
        );
        {
            let mut separated = query.separated(", ");
            for flag_key in flag_keys {
                separated.push_bind(flag_key);
            }
        }
        query.push(") ORDER BY created_ts ASC, id ASC");

        let rows = query
            .build_query_as::<FeatureFlagTargetRecord>()
            .fetch_all(&*self.pool)
            .await?;

        let mut grouped: HashMap<String, Vec<FeatureFlagTargetRecord>> = HashMap::new();
        for row in rows {
            grouped.entry(row.flag_key.clone()).or_default().push(row);
        }
        Ok(grouped)
    }
}

fn to_feature_flag(
    record: FeatureFlagRecord,
    targets: Vec<FeatureFlagTargetRecord>,
) -> FeatureFlag {
    FeatureFlag {
        flag_key: record.flag_key,
        target_scope: record.target_scope,
        rollout_percent: record.rollout_percent,
        expires_at: record.expires_at,
        reason: record.reason,
        status: record.status,
        created_by: record.created_by,
        created_ts: record.created_ts,
        updated_ts: record.updated_ts,
        targets,
    }
}
