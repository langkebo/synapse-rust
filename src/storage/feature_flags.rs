use crate::cache::{CacheManager, InvalidationType};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder};
use std::collections::HashMap;
use std::sync::Arc;

const FEATURE_FLAG_CACHE_TTL_SECS: u64 = 60;
const FEATURE_FLAG_LIST_CACHE_TTL_SECS: u64 = 30;
const FEATURE_FLAG_LIST_CACHE_PREFIX: &str = "feature_flag:list:";

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
    pub cursor_updated_ts: Option<i64>,
    pub cursor_flag_key: Option<String>,
}

#[derive(Clone)]
pub struct FeatureFlagStorage {
    pool: Arc<PgPool>,
    cache: Arc<CacheManager>,
}

impl FeatureFlagStorage {
    pub fn new(pool: &Arc<PgPool>, cache: Arc<CacheManager>) -> Self {
        Self { pool: pool.clone(), cache }
    }

    pub async fn create_flag(
        &self,
        request: &CreateFeatureFlagRequest,
        created_by: &str,
        created_ts: i64,
    ) -> Result<FeatureFlag, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        let record = sqlx::query_as!(FeatureFlagRecord,
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
            RETURNING flag_key as "flag_key!", target_scope as "target_scope!", rollout_percent as "rollout_percent!", expires_at, reason as "reason!", status as "status!", created_by as "created_by!", created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            &request.flag_key,
            &request.target_scope,
            request.rollout_percent,
            request.expires_at,
            &request.reason,
            request.status.as_deref().unwrap_or("draft"),
            created_by,
            created_ts
        )
        .fetch_one(&mut *transaction)
        .await?;

        let targets = self.replace_targets(&mut transaction, &request.flag_key, created_ts, &request.targets).await?;

        transaction.commit().await?;

        let flag = to_feature_flag(record, targets);
        let _ = self.cache.set(&Self::flag_cache_key(&flag.flag_key), &flag, FEATURE_FLAG_CACHE_TTL_SECS).await;
        self.cache.delete_with_invalidation(FEATURE_FLAG_LIST_CACHE_PREFIX, InvalidationType::Prefix).await;

        Ok(flag)
    }

    pub async fn update_flag(
        &self,
        flag_key: &str,
        request: &UpdateFeatureFlagRequest,
        updated_ts: i64,
    ) -> Result<Option<FeatureFlag>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;

        let record = sqlx::query_as!(FeatureFlagRecord,
            r#"
            UPDATE feature_flags
            SET rollout_percent = COALESCE($2, rollout_percent),
                expires_at = COALESCE($3, expires_at),
                reason = COALESCE($4, reason),
                status = COALESCE($5, status),
                updated_ts = $6
            WHERE flag_key = $1
            RETURNING flag_key as "flag_key!", target_scope as "target_scope!", rollout_percent as "rollout_percent!", expires_at, reason as "reason!", status as "status!", created_by as "created_by!", created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            flag_key,
            request.rollout_percent,
            request.expires_at,
            request.reason.as_deref(),
            request.status.as_deref(),
            updated_ts
        )
        .fetch_optional(&mut *transaction)
        .await?;

        let Some(record) = record else {
            transaction.rollback().await?;
            return Ok(None);
        };

        let targets = match &request.targets {
            Some(targets) => self.replace_targets(&mut transaction, flag_key, updated_ts, targets).await?,
            None => Vec::new(),
        };

        transaction.commit().await?;

        let targets = if request.targets.is_some() {
            targets
        } else {
            let mut targets_by_flag = self.list_targets(&[flag_key.to_string()]).await?;
            targets_by_flag.remove(flag_key).unwrap_or_default()
        };

        let flag = to_feature_flag(record, targets);
        let _ = self.cache.set(&Self::flag_cache_key(&flag.flag_key), &flag, FEATURE_FLAG_CACHE_TTL_SECS).await;
        self.cache.delete_with_invalidation(FEATURE_FLAG_LIST_CACHE_PREFIX, InvalidationType::Prefix).await;

        Ok(Some(flag))
    }

    pub async fn get_flag(&self, flag_key: &str) -> Result<Option<FeatureFlag>, sqlx::Error> {
        let cache_key = Self::flag_cache_key(flag_key);
        if let Ok(Some(flag)) = self.cache.get::<FeatureFlag>(&cache_key).await {
            return Ok(Some(flag));
        }

        let record = sqlx::query_as!(FeatureFlagRecord,
            r#"
            SELECT flag_key as "flag_key!", target_scope as "target_scope!", rollout_percent as "rollout_percent!", expires_at, reason as "reason!", status as "status!", created_by as "created_by!", created_ts as "created_ts!", updated_ts as "updated_ts!"
            FROM feature_flags
            WHERE flag_key = $1
            "#,
            flag_key
        )
        .fetch_optional(&*self.pool)
        .await?;

        let Some(record) = record else {
            return Ok(None);
        };

        let targets = self.list_targets(&[flag_key.to_string()]).await?;
        let flag = to_feature_flag(record, targets.get(flag_key).cloned().unwrap_or_default());
        let _ = self.cache.set(&cache_key, &flag, FEATURE_FLAG_CACHE_TTL_SECS).await;

        Ok(Some(flag))
    }

    pub async fn list_flags(&self, filters: &FeatureFlagFilters) -> Result<(Vec<FeatureFlag>, i64), sqlx::Error> {
        let cache_key = Self::flag_list_cache_key(filters);
        if let Ok(Some(cached)) = self.cache.get::<(Vec<FeatureFlag>, i64)>(&cache_key).await {
            return Ok(cached);
        }

        let mut count_query = QueryBuilder::<Postgres>::new("SELECT COUNT(*)::BIGINT FROM feature_flags WHERE 1=1");
        if let Some(ref target_scope) = filters.target_scope {
            count_query.push(" AND target_scope = ");
            count_query.push_bind(target_scope);
        }
        if let Some(ref status) = filters.status {
            count_query.push(" AND status = ");
            count_query.push_bind(status);
        }
        let total = count_query.build_query_scalar::<i64>().fetch_one(&*self.pool).await?;

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
        if let (Some(cursor_updated_ts), Some(cursor_flag_key)) =
            (filters.cursor_updated_ts, filters.cursor_flag_key.as_ref())
        {
            query.push(" AND (updated_ts < ");
            query.push_bind(cursor_updated_ts);
            query.push(" OR (updated_ts = ");
            query.push_bind(cursor_updated_ts);
            query.push(" AND flag_key > ");
            query.push_bind(cursor_flag_key);
            query.push("))");
        }
        query.push(" ORDER BY updated_ts DESC, flag_key ASC LIMIT ");
        query.push_bind(filters.limit);

        let records = query.build_query_as::<FeatureFlagRecord>().fetch_all(&*self.pool).await?;
        let keys: Vec<String> = records.iter().map(|record| record.flag_key.clone()).collect();
        let targets = self.list_targets(&keys).await?;

        let flags = records
            .into_iter()
            .map(|record| {
                let flag_key = record.flag_key.clone();
                to_feature_flag(record, targets.get(&flag_key).cloned().unwrap_or_default())
            })
            .collect();

        let result = (flags, total);
        let _ = self.cache.set(&cache_key, &result, FEATURE_FLAG_LIST_CACHE_TTL_SECS).await;

        Ok(result)
    }

    async fn replace_targets(
        &self,
        transaction: &mut sqlx::Transaction<'_, Postgres>,
        flag_key: &str,
        created_ts: i64,
        targets: &[FeatureFlagTargetInput],
    ) -> Result<Vec<FeatureFlagTargetRecord>, sqlx::Error> {
        sqlx::query!("DELETE FROM feature_flag_targets WHERE flag_key = $1", flag_key)
            .execute(&mut **transaction)
            .await?;

        let mut inserted = Vec::with_capacity(targets.len());
        for target in targets {
            let record = sqlx::query_as!(FeatureFlagTargetRecord,
                r#"
                INSERT INTO feature_flag_targets (
                    flag_key,
                    subject_type,
                    subject_id,
                    created_ts
                )
                VALUES ($1, $2, $3, $4)
                RETURNING id as "id!", flag_key as "flag_key!", subject_type as "subject_type!", subject_id as "subject_id!", created_ts as "created_ts!"
                "#,
                flag_key,
                &target.subject_type,
                &target.subject_id,
                created_ts
            )
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

        let rows = query.build_query_as::<FeatureFlagTargetRecord>().fetch_all(&*self.pool).await?;

        let mut grouped: HashMap<String, Vec<FeatureFlagTargetRecord>> = HashMap::new();
        for row in rows {
            grouped.entry(row.flag_key.clone()).or_default().push(row);
        }
        Ok(grouped)
    }

    fn flag_cache_key(flag_key: &str) -> String {
        format!("feature_flag:{flag_key}")
    }

    fn flag_list_cache_key(filters: &FeatureFlagFilters) -> String {
        format!(
            "{}v1:{}:{}:{}:{}:{}",
            FEATURE_FLAG_LIST_CACHE_PREFIX,
            filters.target_scope.as_deref().unwrap_or("all"),
            filters.status.as_deref().unwrap_or("all"),
            filters.limit,
            filters.cursor_updated_ts.map_or_else(String::new, |ts| ts.to_string()),
            filters.cursor_flag_key.as_deref().unwrap_or(""),
        )
    }
}

fn to_feature_flag(record: FeatureFlagRecord, targets: Vec<FeatureFlagTargetRecord>) -> FeatureFlag {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheConfig;
    use uuid::Uuid;

    fn create_test_cache() -> Arc<CacheManager> {
        Arc::new(CacheManager::new(&CacheConfig::default()))
    }

    async fn create_test_pool() -> Option<Arc<PgPool>> {
        let db_url = crate::test_config::test_database_url();
        let pool = sqlx::PgPool::connect(&db_url).await.ok()?;
        Some(Arc::new(pool))
    }

    async fn ensure_feature_flag_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS feature_flags (
                flag_key TEXT PRIMARY KEY,
                target_scope TEXT NOT NULL,
                rollout_percent INTEGER NOT NULL,
                expires_at BIGINT NULL,
                reason TEXT NOT NULL,
                status TEXT NOT NULL,
                created_by TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                updated_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS feature_flag_targets (
                id BIGSERIAL PRIMARY KEY,
                flag_key TEXT NOT NULL REFERENCES feature_flags(flag_key) ON DELETE CASCADE,
                subject_type TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    async fn cleanup_test_flags(pool: &PgPool, key_prefix: &str) -> Result<(), sqlx::Error> {
        let like_pattern = format!("{key_prefix}%");
        sqlx::query("DELETE FROM feature_flag_targets WHERE flag_key LIKE $1")
            .bind(&like_pattern)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM feature_flags WHERE flag_key LIKE $1").bind(&like_pattern).execute(pool).await?;
        Ok(())
    }

    #[test]
    fn test_flag_list_cache_key_encodes_filters_and_cursor() {
        let filters = FeatureFlagFilters {
            target_scope: Some("tenant".to_string()),
            status: Some("active".to_string()),
            limit: 25,
            cursor_updated_ts: Some(1_700_000_000_000),
            cursor_flag_key: Some("beta.rollout".to_string()),
        };

        assert_eq!(
            FeatureFlagStorage::flag_list_cache_key(&filters),
            "feature_flag:list:v1:tenant:active:25:1700000000000:beta.rollout"
        );
    }

    #[tokio::test]
    async fn test_list_flags_cache_is_invalidated_after_update() {
        let Some(pool) = create_test_pool().await else {
            return;
        };
        ensure_feature_flag_tables(&pool).await.unwrap();

        let cache = create_test_cache();
        let storage = FeatureFlagStorage::new(&pool, cache.clone());
        let test_scope = format!("test-scope-{}", Uuid::new_v4().simple());
        let flag_key_prefix = format!("ff-cache-test-{}", Uuid::new_v4().simple());
        let flag_key_a = format!("{flag_key_prefix}-a");
        let flag_key_b = format!("{flag_key_prefix}-b");
        let created_ts = 1_700_000_000_000_i64;

        cleanup_test_flags(&pool, &flag_key_prefix).await.unwrap();

        storage
            .create_flag(
                &CreateFeatureFlagRequest {
                    flag_key: flag_key_a.clone(),
                    target_scope: test_scope.clone(),
                    rollout_percent: 10,
                    expires_at: None,
                    reason: "test cache invalidation".to_string(),
                    status: Some("draft".to_string()),
                    targets: vec![FeatureFlagTargetInput {
                        subject_type: "user".to_string(),
                        subject_id: "@alice:test".to_string(),
                    }],
                },
                "@tester:test",
                created_ts,
            )
            .await
            .unwrap();

        storage
            .create_flag(
                &CreateFeatureFlagRequest {
                    flag_key: flag_key_b.clone(),
                    target_scope: test_scope.clone(),
                    rollout_percent: 20,
                    expires_at: None,
                    reason: "test cache invalidation".to_string(),
                    status: Some("draft".to_string()),
                    targets: vec![FeatureFlagTargetInput {
                        subject_type: "user".to_string(),
                        subject_id: "@bob:test".to_string(),
                    }],
                },
                "@tester:test",
                created_ts + 1,
            )
            .await
            .unwrap();

        let filters = FeatureFlagFilters {
            target_scope: Some(test_scope.clone()),
            status: Some("draft".to_string()),
            limit: 10,
            cursor_updated_ts: None,
            cursor_flag_key: None,
        };
        let cache_key = FeatureFlagStorage::flag_list_cache_key(&filters);

        assert!(cache.get_local_raw(&cache_key).is_none());

        let (flags, total) = storage.list_flags(&filters).await.unwrap();
        assert_eq!(total, 2);
        assert_eq!(flags.len(), 2);
        assert!(cache.get_local_raw(&cache_key).is_some());

        storage
            .update_flag(
                &flag_key_a,
                &UpdateFeatureFlagRequest { status: Some("active".to_string()), ..Default::default() },
                created_ts + 2,
            )
            .await
            .unwrap()
            .unwrap();

        assert!(cache.get_local_raw(&cache_key).is_none());

        let (flags_after_update, total_after_update) = storage.list_flags(&filters).await.unwrap();
        assert_eq!(total_after_update, 1);
        assert_eq!(flags_after_update.len(), 1);
        assert_eq!(flags_after_update[0].flag_key, flag_key_b);
        assert!(cache.get_local_raw(&cache_key).is_some());

        cleanup_test_flags(&pool, &flag_key_prefix).await.unwrap();
    }
}
