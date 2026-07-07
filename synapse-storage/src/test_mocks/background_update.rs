use super::*;

pub struct InMemoryBackgroundUpdateStore {
    updates: tokio::sync::RwLock<HashMap<String, BackgroundUpdate>>,
    locks: tokio::sync::RwLock<HashMap<String, bool>>,
    history: tokio::sync::RwLock<Vec<BackgroundUpdateHistory>>,
    next_history_id: tokio::sync::RwLock<i64>,
}

impl Default for InMemoryBackgroundUpdateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryBackgroundUpdateStore {
    pub fn new() -> Self {
        Self {
            updates: tokio::sync::RwLock::new(HashMap::new()),
            locks: tokio::sync::RwLock::new(HashMap::new()),
            history: tokio::sync::RwLock::new(Vec::new()),
            next_history_id: tokio::sync::RwLock::new(1),
        }
    }
}

#[async_trait::async_trait]
impl BackgroundUpdateStoreApi for InMemoryBackgroundUpdateStore {
    async fn create_update(&self, request: CreateBackgroundUpdateRequest) -> Result<BackgroundUpdate, sqlx::Error> {
        let now = Utc::now().timestamp_millis();
        let update = BackgroundUpdate {
            job_name: request.job_name.clone(),
            job_type: request.job_type,
            description: request.description,
            table_name: request.table_name,
            column_name: request.column_name,
            status: "pending".to_string(),
            progress: serde_json::json!(0),
            total_items: request.total_items.unwrap_or(0),
            processed_items: 0,
            created_ts: now,
            started_ts: None,
            completed_ts: None,
            updated_ts: None,
            error_message: None,
            retry_count: 0,
            max_retries: 3,
            batch_size: request.batch_size.unwrap_or(100),
            sleep_ms: request.sleep_ms.unwrap_or(1000),
            depends_on: request
                .depends_on
                .map(|deps| serde_json::Value::Array(deps.into_iter().map(serde_json::Value::String).collect())),
            metadata: request.metadata,
        };
        self.updates.write().await.insert(update.job_name.clone(), update.clone());
        Ok(update)
    }

    async fn get_update(&self, job_name: &str) -> Result<Option<BackgroundUpdate>, sqlx::Error> {
        Ok(self.updates.read().await.get(job_name).cloned())
    }

    async fn get_all_updates(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<BackgroundUpdate>, Option<String>), sqlx::Error> {
        let updates = self.updates.read().await;
        let mut sorted: Vec<BackgroundUpdate> = updates.values().cloned().collect();
        sorted.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.job_name.cmp(&a.job_name)));
        let from_idx = from
            .as_deref()
            .and_then(|cursor| {
                let (ts, name) = cursor.split_once('|')?;
                let ts = ts.parse::<i64>().ok()?;
                sorted.iter().position(|u| u.created_ts == ts && u.job_name == name)
            })
            .map(|p| p + 1)
            .unwrap_or(0);
        let page: Vec<BackgroundUpdate> = sorted.into_iter().skip(from_idx).take(limit as usize).collect();
        let next = if page.len() as i64 == limit {
            page.last().map(|u| format!("{}|{}", u.created_ts, u.job_name))
        } else {
            None
        };
        Ok((page, next))
    }

    async fn get_pending_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        Ok(self.updates.read().await.values().filter(|u| u.status == "pending").cloned().collect())
    }

    async fn get_running_updates(&self) -> Result<Vec<BackgroundUpdate>, sqlx::Error> {
        Ok(self.updates.read().await.values().filter(|u| u.status == "running").cloned().collect())
    }

    async fn update_status(&self, job_name: &str, status: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let update = updates.get_mut(job_name).ok_or_else(|| sqlx::Error::RowNotFound)?;
        let now = Utc::now().timestamp_millis();
        update.status = status.to_string();
        update.updated_ts = Some(now);
        if status == "running" {
            update.started_ts = Some(now);
        }
        if status == "completed" {
            update.completed_ts = Some(now);
        }
        Ok(update.clone())
    }

    async fn update_progress(
        &self,
        job_name: &str,
        items_processed: i32,
        total_items: Option<i32>,
    ) -> Result<BackgroundUpdate, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let update = updates.get_mut(job_name).ok_or_else(|| sqlx::Error::RowNotFound)?;
        update.processed_items += items_processed;
        if let Some(t) = total_items {
            update.total_items = t;
        }
        if update.total_items > 0 {
            update.progress =
                serde_json::json!(((update.processed_items as f64 / update.total_items as f64) * 100.0).round() as i64);
        }
        update.updated_ts = Some(Utc::now().timestamp_millis());
        Ok(update.clone())
    }

    async fn set_error(&self, job_name: &str, error_message: &str) -> Result<BackgroundUpdate, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let update = updates.get_mut(job_name).ok_or_else(|| sqlx::Error::RowNotFound)?;
        update.status = "failed".to_string();
        update.error_message = Some(error_message.to_string());
        update.updated_ts = Some(Utc::now().timestamp_millis());
        update.retry_count += 1;
        Ok(update.clone())
    }

    async fn delete_update(&self, job_name: &str) -> Result<(), sqlx::Error> {
        self.updates.write().await.remove(job_name);
        Ok(())
    }

    async fn acquire_lock_with_retry(
        &self,
        job_name: &str,
        _locked_by: &str,
        _lock_duration_ms: i64,
        _max_retries: u32,
        _max_retry_interval_ms: u64,
    ) -> Result<bool, sqlx::Error> {
        let mut locks = self.locks.write().await;
        if locks.get(job_name).copied().unwrap_or(false) {
            Ok(false)
        } else {
            locks.insert(job_name.to_string(), true);
            Ok(true)
        }
    }

    async fn release_lock(&self, job_name: &str) -> Result<(), sqlx::Error> {
        self.locks.write().await.remove(job_name);
        Ok(())
    }

    async fn is_locked(&self, job_name: &str) -> Result<bool, sqlx::Error> {
        Ok(self.locks.read().await.get(job_name).copied().unwrap_or(false))
    }

    async fn cleanup_expired_locks(&self) -> Result<i64, sqlx::Error> {
        let count = self.locks.read().await.len() as i64;
        self.locks.write().await.clear();
        Ok(count)
    }

    async fn add_history(
        &self,
        job_name: &str,
        status: &str,
        items_processed: i32,
        error_message: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<BackgroundUpdateHistory, sqlx::Error> {
        let mut next_id = self.next_history_id.write().await;
        let id = *next_id;
        *next_id += 1;
        let now = Utc::now().timestamp_millis();
        let entry = BackgroundUpdateHistory {
            id,
            job_name: job_name.to_string(),
            execution_start_ts: now,
            execution_end_ts: Some(now),
            status: status.to_string(),
            items_processed,
            error_message: error_message.map(|s| s.to_string()),
            metadata,
        };
        self.history.write().await.push(entry.clone());
        Ok(entry)
    }

    async fn get_history(&self, job_name: &str, limit: i64) -> Result<Vec<BackgroundUpdateHistory>, sqlx::Error> {
        let mut entries: Vec<BackgroundUpdateHistory> =
            self.history.read().await.iter().filter(|h| h.job_name == job_name).cloned().collect();
        entries.sort_by(|a, b| b.execution_start_ts.cmp(&a.execution_start_ts));
        entries.truncate(limit as usize);
        Ok(entries)
    }

    async fn retry_failed(&self) -> Result<i64, sqlx::Error> {
        let mut updates = self.updates.write().await;
        let mut count = 0i64;
        for update in updates.values_mut() {
            if update.status == "failed" && update.retry_count < update.max_retries {
                update.status = "pending".to_string();
                update.error_message = None;
                update.retry_count += 1;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn count_by_status(&self, status: &str) -> Result<i64, sqlx::Error> {
        Ok(self.updates.read().await.values().filter(|u| u.status == status).count() as i64)
    }

    async fn count_all(&self) -> Result<i64, sqlx::Error> {
        Ok(self.updates.read().await.len() as i64)
    }

    async fn get_stats(&self, _limit: i32) -> Result<Vec<BackgroundUpdateStats>, sqlx::Error> {
        Ok(Vec::new())
    }
}
