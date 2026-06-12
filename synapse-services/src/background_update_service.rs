use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::background_update::*;
use tracing::{info, instrument, warn};
use uuid::Uuid;

pub struct BackgroundUpdateService {
    storage: Arc<BackgroundUpdateStorage>,
}

impl BackgroundUpdateService {
    pub fn new(storage: Arc<BackgroundUpdateStorage>) -> Self {
        Self { storage }
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

        let locked = self
            .storage
            .acquire_lock(job_name, &Uuid::new_v4().to_string(), 300000)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to acquire lock", &e))?;

        if !locked {
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
