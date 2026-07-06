use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::background_update::*;
use tracing::{info, instrument, warn};
use uuid::Uuid;

/// Default lock retry parameters. These can be overridden via WorkerConfig.
const DEFAULT_LOCK_MAX_RETRIES: u32 = 3;
const DEFAULT_LOCK_MAX_RETRY_INTERVAL_MS: u64 = 5000;

pub struct BackgroundUpdateService {
    storage: Arc<dyn BackgroundUpdateStoreApi>,
    /// Maximum retry attempts for lock acquisition (from WorkerConfig).
    lock_max_retries: u32,
    /// Maximum interval between lock retries in ms (from WorkerConfig).
    lock_max_retry_interval_ms: u64,
}

impl BackgroundUpdateService {
    pub fn new(storage: Arc<dyn BackgroundUpdateStoreApi>) -> Self {
        Self {
            storage,
            lock_max_retries: DEFAULT_LOCK_MAX_RETRIES,
            lock_max_retry_interval_ms: DEFAULT_LOCK_MAX_RETRY_INTERVAL_MS,
        }
    }

    /// Configure lock retry parameters from WorkerConfig.
    ///
    /// Aligned with Synapse v1.153.0 which lowered
    /// `WORKER_LOCK_MAX_RETRY_INTERVAL` to 5 seconds to reduce lock
    /// contention CPU starvation.
    pub fn with_lock_retry_config(mut self, max_retries: u32, max_retry_interval_ms: u64) -> Self {
        self.lock_max_retries = max_retries;
        self.lock_max_retry_interval_ms = max_retry_interval_ms;
        self
    }

    #[instrument(skip(self))]
    pub async fn create_update(&self, request: CreateBackgroundUpdateRequest) -> Result<BackgroundUpdate, ApiError> {
        info!(job_name = %request.job_name, "Creating background update");

        if self
            .storage
            .get_update(&request.job_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check update", &e))?
            .is_some()
        {
            return Err(ApiError::bad_request("Update job already exists"));
        }

        let update = self
            .storage
            .create_update(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create update", &e))?;

        info!(job_name = %update.job_name, status = %update.status, "Created background update");

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn get_update(&self, job_name: &str) -> Result<Option<BackgroundUpdate>, ApiError> {
        let update = self
            .storage
            .get_update(job_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get update", &e))?;

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn get_all_updates(
        &self,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<BackgroundUpdate>, Option<String>), ApiError> {
        let updates = self
            .storage
            .get_all_updates(limit, from)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get updates", &e))?;

        Ok(updates)
    }

    #[instrument(skip(self))]
    pub async fn get_pending_updates(&self) -> Result<Vec<BackgroundUpdate>, ApiError> {
        let updates = self
            .storage
            .get_pending_updates()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending updates", &e))?;

        Ok(updates)
    }

    #[instrument(skip(self))]
    pub async fn get_running_updates(&self) -> Result<Vec<BackgroundUpdate>, ApiError> {
        let updates = self
            .storage
            .get_running_updates()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get running updates", &e))?;

        Ok(updates)
    }

    #[instrument(skip(self))]
    pub async fn start_update(&self, job_name: &str) -> Result<BackgroundUpdate, ApiError> {
        info!(job_name = %job_name, "Starting background update");

        let update = self
            .storage
            .get_update(job_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get update", &e))?
            .ok_or_else(|| ApiError::not_found("Update not found"))?;

        if update.status != "pending" {
            return Err(ApiError::bad_request("Update is not in pending status"));
        }

        // OPT-09: Use configurable exponential backoff retry for lock
        // acquisition. This prevents CPU starvation under lock contention
        // while still allowing workers to eventually acquire the lock.
        // Aligned with Synapse v1.153.0 WORKER_LOCK_MAX_RETRY_INTERVAL.
        let lock_start = std::time::Instant::now();
        let locked = self
            .storage
            .acquire_lock_with_retry(
                job_name,
                &Uuid::new_v4().to_string(),
                300000,
                self.lock_max_retries,
                self.lock_max_retry_interval_ms,
            )
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to acquire lock", &e))?;

        let lock_wait_ms = lock_start.elapsed().as_millis();
        if locked {
            info!(job_name = %job_name, lock_wait_ms = lock_wait_ms, "Acquired background update lock");
        } else {
            warn!(
                job_name = %job_name,
                lock_wait_ms = lock_wait_ms,
                max_retries = self.lock_max_retries,
                max_retry_interval_ms = self.lock_max_retry_interval_ms,
                "Failed to acquire background update lock after retries"
            );
            return Err(ApiError::bad_request("Failed to acquire lock, job may be locked by another process"));
        }

        let update = self
            .storage
            .update_status(job_name, "running")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to start update", &e))?;

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn update_progress(
        &self,
        job_name: &str,
        items_processed: i32,
        total_items: Option<i32>,
    ) -> Result<BackgroundUpdate, ApiError> {
        let update = self
            .storage
            .update_progress(job_name, items_processed, total_items)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update progress", &e))?;

        let progress_value = update.progress.as_i64().unwrap_or(0);
        if progress_value >= 100 || (update.total_items > 0 && update.processed_items >= update.total_items) {
            self.complete_update(job_name).await?;
        }

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn complete_update(&self, job_name: &str) -> Result<BackgroundUpdate, ApiError> {
        info!(job_name = %job_name, "Completing background update");

        let update = self
            .storage
            .update_status(job_name, "completed")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to complete update", &e))?;

        self.storage.release_lock(job_name).await.ok();

        self.storage.add_history(job_name, "completed", update.processed_items, None, None).await.ok();

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn fail_update(&self, job_name: &str, error_message: &str) -> Result<BackgroundUpdate, ApiError> {
        warn!(
            job_name = %job_name,
            error_message_present = !error_message.is_empty(),
            error_message_len = error_message.len(),
            "Failing background update"
        );

        let update = self
            .storage
            .set_error(job_name, error_message)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to fail update", &e))?;

        self.storage.release_lock(job_name).await.ok();

        self.storage.add_history(job_name, "failed", update.processed_items, Some(error_message), None).await.ok();

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn cancel_update(&self, job_name: &str) -> Result<BackgroundUpdate, ApiError> {
        info!(job_name = %job_name, "Cancelling background update");

        let update = self
            .storage
            .update_status(job_name, "cancelled")
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cancel update", &e))?;

        self.storage.release_lock(job_name).await.ok();

        Ok(update)
    }

    #[instrument(skip(self))]
    pub async fn delete_update(&self, job_name: &str) -> Result<(), ApiError> {
        self.storage
            .delete_update(job_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete update", &e))?;

        info!(job_name = %job_name, "Deleted background update");

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn retry_failed(&self) -> Result<i64, ApiError> {
        info!("Retrying failed background updates");

        let count = self
            .storage
            .retry_failed()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to retry updates", &e))?;

        info!(retried_count = count, "Retried failed background updates");

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn cleanup_expired_locks(&self) -> Result<i64, ApiError> {
        info!("Cleaning up expired locks");

        let count = self
            .storage
            .cleanup_expired_locks()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup locks", &e))?;

        info!(expired_lock_count = count, "Cleaned up expired locks");

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn get_history(&self, job_name: &str, limit: i64) -> Result<Vec<BackgroundUpdateHistory>, ApiError> {
        let history = self
            .storage
            .get_history(job_name, limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get history", &e))?;

        Ok(history)
    }

    #[instrument(skip(self))]
    pub async fn count_by_status(&self, status: &str) -> Result<i64, ApiError> {
        let count = self
            .storage
            .count_by_status(status)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to count updates", &e))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn count_all(&self) -> Result<i64, ApiError> {
        let count =
            self.storage.count_all().await.map_err(|e| ApiError::internal_with_log("Failed to count updates", &e))?;

        Ok(count)
    }

    #[instrument(skip(self))]
    pub async fn get_stats(&self, days: i32) -> Result<Vec<BackgroundUpdateStats>, ApiError> {
        let stats =
            self.storage.get_stats(days).await.map_err(|e| ApiError::internal_with_log("Failed to get stats", &e))?;

        Ok(stats)
    }

    #[instrument(skip(self))]
    pub async fn is_locked(&self, job_name: &str) -> Result<bool, ApiError> {
        let locked = self
            .storage
            .is_locked(job_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check lock", &e))?;

        Ok(locked)
    }

    pub async fn get_next_pending_update(&self) -> Result<Option<BackgroundUpdate>, ApiError> {
        let pending = self.get_pending_updates().await?;

        for update in pending {
            if let Some(ref depends_on) = update.depends_on {
                let mut all_completed = true;
                if let Some(deps) = depends_on.as_array() {
                    for dep_value in deps {
                        if let Some(dep) = dep_value.as_str() {
                            if let Some(dep_update) = self
                                .storage
                                .get_update(dep)
                                .await
                                .map_err(|e| ApiError::internal_with_log("Failed to check dependency", &e))?
                            {
                                if dep_update.status != "completed" {
                                    all_completed = false;
                                    break;
                                }
                            }
                        }
                    }
                }
                if !all_completed {
                    continue;
                }
            }

            let locked = self
                .storage
                .is_locked(&update.job_name)
                .await
                .map_err(|e| ApiError::internal_with_log("Failed to check lock", &e))?;

            if !locked {
                return Ok(Some(update));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_storage::test_mocks::InMemoryBackgroundUpdateStore;

    fn test_service() -> BackgroundUpdateService {
        BackgroundUpdateService::new(Arc::new(InMemoryBackgroundUpdateStore::new()))
    }

    fn create_request(job_name: &str) -> CreateBackgroundUpdateRequest {
        CreateBackgroundUpdateRequest {
            job_name: job_name.to_string(),
            job_type: "test".to_string(),
            description: None,
            table_name: None,
            column_name: None,
            total_items: None,
            batch_size: None,
            sleep_ms: None,
            depends_on: None,
            metadata: None,
        }
    }

    // ── create_update ──────────────────────────────────────────────

    #[tokio::test]
    async fn create_update_stores_pending_update() {
        let svc = test_service();
        let req = create_request("job1");
        let update = svc.create_update(req).await.unwrap();
        assert_eq!(update.job_name, "job1");
        assert_eq!(update.status, "pending");
    }

    #[tokio::test]
    async fn create_update_fails_on_duplicate_job_name() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        let err = svc.create_update(create_request("job1")).await.unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    // ── start_update ───────────────────────────────────────────────

    #[tokio::test]
    async fn start_update_transitions_pending_to_running() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        let update = svc.start_update("job1").await.unwrap();
        assert_eq!(update.status, "running");
    }

    #[tokio::test]
    async fn start_update_fails_when_not_found() {
        let svc = test_service();
        let err = svc.start_update("nonexistent").await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn start_update_fails_when_not_pending() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.start_update("job1").await.unwrap(); // now running
        let err = svc.start_update("job1").await.unwrap_err();
        assert!(err.to_string().contains("not in pending status"));
    }

    #[tokio::test]
    async fn start_update_fails_when_locked() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        // Acquire the lock first
        svc.storage.acquire_lock_with_retry("job1", "other", 300000, 0, 0).await.unwrap();
        let err = svc.start_update("job1").await.unwrap_err();
        assert!(err.to_string().contains("Failed to acquire lock"));
    }

    // ── update_progress ────────────────────────────────────────────

    #[tokio::test]
    async fn update_progress_updates_items() {
        let svc = test_service();
        svc.create_update(CreateBackgroundUpdateRequest { total_items: Some(100), ..create_request("job1") })
            .await
            .unwrap();
        svc.start_update("job1").await.unwrap();
        let update = svc.update_progress("job1", 50, None).await.unwrap();
        assert_eq!(update.processed_items, 50);
    }

    #[tokio::test]
    async fn update_progress_auto_completes_when_processed_equals_total() {
        let svc = test_service();
        svc.create_update(CreateBackgroundUpdateRequest { total_items: Some(10), ..create_request("job1") })
            .await
            .unwrap();
        svc.start_update("job1").await.unwrap();
        svc.update_progress("job1", 10, None).await.unwrap();
        // Side effect: the update is now completed in storage
        let stored = svc.get_update("job1").await.unwrap().unwrap();
        assert_eq!(stored.status, "completed");
        assert!(!svc.storage.is_locked("job1").await.unwrap());
    }

    // ── complete_update ────────────────────────────────────────────

    #[tokio::test]
    async fn complete_update_changes_status_and_releases_lock() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.start_update("job1").await.unwrap();
        let update = svc.complete_update("job1").await.unwrap();
        assert_eq!(update.status, "completed");
        assert!(!svc.storage.is_locked("job1").await.unwrap());
    }

    #[tokio::test]
    async fn complete_update_adds_history() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.start_update("job1").await.unwrap();
        svc.complete_update("job1").await.unwrap();
        let history = svc.get_history("job1", 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "completed");
    }

    // ── fail_update ────────────────────────────────────────────────

    #[tokio::test]
    async fn fail_update_sets_error_and_releases_lock() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.start_update("job1").await.unwrap();
        let update = svc.fail_update("job1", "something broke").await.unwrap();
        assert_eq!(update.status, "failed");
        assert_eq!(update.error_message.as_deref(), Some("something broke"));
        assert!(!svc.storage.is_locked("job1").await.unwrap());
    }

    // ── cancel_update ──────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_update_changes_status_and_releases_lock() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.start_update("job1").await.unwrap();
        let update = svc.cancel_update("job1").await.unwrap();
        assert_eq!(update.status, "cancelled");
        assert!(!svc.storage.is_locked("job1").await.unwrap());
    }

    // ── get_next_pending_update ────────────────────────────────────

    #[tokio::test]
    async fn get_next_pending_returns_unlocked_pending_update() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        let next = svc.get_next_pending_update().await.unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().job_name, "job1");
    }

    #[tokio::test]
    async fn get_next_pending_skips_locked_update() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.create_update(create_request("job2")).await.unwrap();
        // Lock job1
        svc.storage.acquire_lock_with_retry("job1", "other", 300000, 0, 0).await.unwrap();
        let next = svc.get_next_pending_update().await.unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().job_name, "job2");
    }

    #[tokio::test]
    async fn get_next_pending_skips_when_dependency_not_completed() {
        let svc = test_service();
        svc.create_update(create_request("dep")).await.unwrap();
        svc.create_update(CreateBackgroundUpdateRequest {
            depends_on: Some(vec!["dep".to_string()]),
            ..create_request("job1")
        })
        .await
        .unwrap();
        // dep is still pending → job1 should be skipped
        let next = svc.get_next_pending_update().await.unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().job_name, "dep");
    }

    #[tokio::test]
    async fn get_next_pending_returns_when_dependency_is_completed() {
        let svc = test_service();
        svc.create_update(create_request("dep")).await.unwrap();
        svc.create_update(CreateBackgroundUpdateRequest {
            depends_on: Some(vec!["dep".to_string()]),
            ..create_request("job1")
        })
        .await
        .unwrap();
        // Complete the dependency
        svc.start_update("dep").await.unwrap();
        svc.complete_update("dep").await.unwrap();
        // Now job1 should be available
        let next = svc.get_next_pending_update().await.unwrap();
        assert!(next.is_some());
        assert_eq!(next.unwrap().job_name, "job1");
    }

    #[tokio::test]
    async fn get_next_pending_returns_none_when_all_locked() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.storage.acquire_lock_with_retry("job1", "other", 300000, 0, 0).await.unwrap();
        let next = svc.get_next_pending_update().await.unwrap();
        assert!(next.is_none());
    }

    // ── passthrough methods ────────────────────────────────────────

    #[tokio::test]
    async fn get_update_returns_stored_update() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        let found = svc.get_update("job1").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().job_name, "job1");
    }

    #[tokio::test]
    async fn get_update_returns_none_for_unknown() {
        let svc = test_service();
        assert!(svc.get_update("unknown").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_all_updates_returns_page() {
        let svc = test_service();
        svc.create_update(create_request("a")).await.unwrap();
        svc.create_update(create_request("b")).await.unwrap();
        let (updates, _next) = svc.get_all_updates(10, None).await.unwrap();
        assert_eq!(updates.len(), 2);
    }

    #[tokio::test]
    async fn delete_update_removes_it() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.delete_update("job1").await.unwrap();
        assert!(svc.get_update("job1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn retry_failed_resets_to_pending() {
        let svc = test_service();
        svc.create_update(create_request("job1")).await.unwrap();
        svc.start_update("job1").await.unwrap();
        svc.fail_update("job1", "error").await.unwrap();
        let count = svc.retry_failed().await.unwrap();
        assert_eq!(count, 1);
        let update = svc.get_update("job1").await.unwrap().unwrap();
        assert_eq!(update.status, "pending");
    }

    #[tokio::test]
    async fn cleanup_expired_locks_returns_count() {
        let svc = test_service();
        svc.storage.acquire_lock_with_retry("a", "x", 300000, 0, 0).await.unwrap();
        svc.storage.acquire_lock_with_retry("b", "x", 300000, 0, 0).await.unwrap();
        let count = svc.cleanup_expired_locks().await.unwrap();
        assert_eq!(count, 2);
        assert!(!svc.storage.is_locked("a").await.unwrap());
    }

    #[tokio::test]
    async fn count_by_status_returns_accurate_count() {
        let svc = test_service();
        svc.create_update(create_request("a")).await.unwrap();
        svc.create_update(create_request("b")).await.unwrap();
        svc.start_update("a").await.unwrap();
        assert_eq!(svc.count_by_status("pending").await.unwrap(), 1);
        assert_eq!(svc.count_by_status("running").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn count_all_returns_total() {
        let svc = test_service();
        svc.create_update(create_request("a")).await.unwrap();
        svc.create_update(create_request("b")).await.unwrap();
        assert_eq!(svc.count_all().await.unwrap(), 2);
    }
}
