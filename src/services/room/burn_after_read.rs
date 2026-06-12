//! Burn-after-read message processing.

use crate::common::background_job::BackgroundJob;
use crate::common::constants::BURN_AFTER_READ_DELAY_SECS;
use crate::common::error::ApiResult;
use std::time::Duration;

use super::service::RoomService;

impl RoomService {
    pub async fn process_read_receipt(
        &self,
        room_id: &str,
        event_id: &str,
        _user_id: &str,
        _custom_delay_secs: Option<u64>,
    ) -> ApiResult<()> {
        let event = match self.event_storage.get_event(event_id).await {
            Ok(Some(e)) => e,
            _ => return Ok(()),
        };

        let content = match event.content.as_object() {
            Some(c) => c,
            None => return Ok(()),
        };

        if !content.contains_key("burn_after_read") {
            return Ok(());
        }

        let queue = match self.task_queue.clone() {
            Some(q) => q,
            None => return Ok(()),
        };

        // Read custom delay time from message content
        let delay_secs = content
            .get("burn_after_read_delay_seconds")
            .and_then(|v| v.as_i64())
            .map_or(BURN_AFTER_READ_DELAY_SECS, |v| v as u64);

        let rid = room_id.to_string();
        let eid = event_id.to_string();
        let task_id = format!("burn_after_read:{rid}:{eid}:{delay_secs}");

        ::tracing::info!(
            room_id = %rid,
            event_id = %eid,
            task_id = %task_id,
            delay_secs,
            "Scheduling burn-after-read"
        );

        // Track spawned task to prevent memory leaks
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(delay_secs)).await;

            let job = BackgroundJob::RedactEvent {
                event_id: eid.clone(),
                room_id: rid.clone(),
                reason: Some("Burn after read".to_string()),
            };

            match queue.submit(job).await {
                Ok(_) => {
                    ::tracing::info!(room_id = %rid, event_id = %eid, delay_secs, "Submitted redaction job");
                }
                Err(e) => {
                    ::tracing::error!(
                        room_id = %rid,
                        event_id = %eid,
                        delay_secs,
                        error = %e,
                        "Failed to submit redaction job"
                    );
                }
            }
        });

        // Store the task handle for later cleanup/management
        self.active_tasks.write().await.insert(task_id, handle);

        Ok(())
    }
}
