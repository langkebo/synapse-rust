use super::*;

#[derive(Default)]
pub struct InMemoryRateLimitStore {
    limits: Arc<RwLock<HashMap<String, crate::rate_limit::RateLimitRecord>>>,
}

impl InMemoryRateLimitStore {
    pub fn new() -> Self {
        Self { limits: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl RateLimitStoreApi for InMemoryRateLimitStore {
    async fn get_user_rate_limit(
        &self,
        user_id: &str,
    ) -> Result<Option<crate::rate_limit::RateLimitRecord>, sqlx::Error> {
        Ok(self.limits.read().await.get(user_id).cloned())
    }

    async fn upsert_user_rate_limit(
        &self,
        user_id: &str,
        messages_per_second: f64,
        burst_count: i32,
    ) -> Result<(), sqlx::Error> {
        self.limits.write().await.insert(
            user_id.to_string(),
            crate::rate_limit::RateLimitRecord {
                messages_per_second: Some(messages_per_second),
                burst_count: Some(burst_count),
            },
        );
        Ok(())
    }

    async fn delete_user_rate_limit(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.limits.write().await.remove(user_id);
        Ok(())
    }
}
