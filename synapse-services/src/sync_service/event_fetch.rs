use super::types::*;
use super::SyncService;
use crate::map_internal;
use crate::*;
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::*;
use synapse_storage::event::SinceFilter;
use tokio::sync::Notify;

impl SyncService {
    pub(crate) async fn fetch_events(
        &self,
        request: FetchEventsRequest<'_>,
    ) -> ApiResult<HashMap<String, Vec<RoomEvent>>> {
        let FetchEventsRequest {
            user_id,
            device_id,
            room_ids,
            since_token,
            timeout,
            limit,
            timeline_filter,
            is_incremental,
        } = request;
        let event_filter = Self::event_query_filter_from_sync_filter(timeline_filter);
        let fetch_limit = if limit <= 0 { 1 } else { limit.saturating_add(1) };

        if is_incremental {
            // S6: always use StreamOrdering. Timestamp-based tokens (stream_id
            // >= 1e12) and invalid tokens (stream_id <= 0) are converted to 0
            // for a full resync, eliminating the OriginServerTs path that has
            // same-millisecond race conditions.
            let stream_ord = since_token
                .as_ref()
                .filter(|t| t.stream_id > 0 && t.stream_id < Self::TIMESTAMP_TOKEN_MIN)
                .map(|t| t.stream_id)
                .unwrap_or(0);

            let events = match event_filter.as_ref() {
                Some(filter) => self
                    .event_reader
                    .get_room_events_batch_since_filtered(
                        room_ids,
                        SinceFilter::StreamOrdering(stream_ord),
                        fetch_limit,
                        filter,
                    )
                    .await?,
                None => self
                    .event_reader
                    .get_room_events_batch_since(room_ids, SinceFilter::StreamOrdering(stream_ord), fetch_limit)
                    .await?,
            };

            if events.values().all(|v| v.is_empty()) && timeout > 0 {
                let update = self
                    .wait_for_incremental_update(user_id, device_id, room_ids, stream_ord, since_token, timeout)
                    .await?;

                match update {
                    IncrementalUpdate::Events => match event_filter.as_ref() {
                        Some(filter) => self
                            .event_reader
                            .get_room_events_batch_since_filtered(
                                room_ids,
                                SinceFilter::StreamOrdering(stream_ord),
                                fetch_limit,
                                filter,
                            )
                            .await
                            .map_err(Into::into),
                        None => self
                            .event_reader
                            .get_room_events_batch_since(
                                room_ids,
                                SinceFilter::StreamOrdering(stream_ord),
                                fetch_limit,
                            )
                            .await
                            .map_err(Into::into),
                    },
                    IncrementalUpdate::Timeout | IncrementalUpdate::ToDevice | IncrementalUpdate::DeviceLists => {
                        Ok(events)
                    }
                }
            } else {
                Ok(events)
            }
        } else {
            match event_filter.as_ref() {
                Some(filter) => self
                    .event_reader
                    .get_room_events_batch_filtered(room_ids, fetch_limit, filter)
                    .await
                    .map_err(Into::into),
                None => self.event_reader.get_room_events_batch(room_ids, fetch_limit).await.map_err(Into::into),
            }
        }
    }

    /// Wait for new data to arrive, using event-driven wake-up when an
    /// [`EventNotifier`] is wired, or falling back to periodic polling
    /// otherwise.
    ///
    /// # Ordering contract
    ///
    /// When an `EventNotifier` is available, notify slots are registered
    /// (via `Notified::enable()`) **before** reading the database. This
    /// ensures a notification fired between the read and the await is not
    /// lost (`Notify::notify_waiters` stores no permit). Producers
    /// ([`NotifyingEventWriter`]) write to the DB first, then notify — so
    /// any event invisible to the read is guaranteed to notify an
    /// already-registered waiter.
    pub(crate) async fn wait_for_incremental_update(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_ids: &[String],
        since_stream_ord: i64,
        since_token: Option<&SyncToken>,
        timeout: u64,
    ) -> ApiResult<IncrementalUpdate> {
        let timeout_duration = std::time::Duration::from_millis(timeout);
        let start = std::time::Instant::now();

        let since_to_device = since_token.and_then(|t| t.to_device_stream_id).unwrap_or(0);
        let since_device_lists = since_token.and_then(|t| t.device_list_stream_id).unwrap_or(0);

        // Pre-fetch notify slots once. Arc<Notify> instances are stable
        // across calls (lazily created, never evicted), so we can reuse
        // them for every loop iteration.
        let notify_slots: Vec<Arc<Notify>> = match self.event_notifier.as_ref() {
            Some(notifier) => notifier.slots_for(user_id, room_ids),
            None => Vec::new(),
        };

        loop {
            // Register waiters BEFORE reading DB (ordering contract).
            // Each iteration re-creates Notified futures from the same
            // Arc<Notify> slots, because Notified is a one-shot future.
            let mut long_poll_waiters: Vec<_> = notify_slots
                .iter()
                .map(|slot| {
                    let mut waiter = Box::pin(slot.notified());
                    waiter.as_mut().enable();
                    waiter
                })
                .collect();

            // Check DB for incremental updates
            let (has_events, has_to_device, has_device_lists) = tokio::try_join!(
                self.has_incremental_room_updates(room_ids, since_stream_ord),
                self.has_incremental_to_device_updates(user_id, device_id, since_to_device),
                self.has_incremental_device_list_updates(since_device_lists),
            )?;

            if has_events {
                return Ok(IncrementalUpdate::Events);
            }
            if has_to_device {
                return Ok(IncrementalUpdate::ToDevice);
            }
            if has_device_lists {
                return Ok(IncrementalUpdate::DeviceLists);
            }

            let remaining = timeout_duration.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Ok(IncrementalUpdate::Timeout);
            }

            if long_poll_waiters.is_empty() {
                // No notifier wired (tests, benchmarks): degrade to
                // periodic polling. Sleep for the lesser of the poll
                // interval or the remaining timeout.
                let poll_interval = self.sync_poll_interval();
                tokio::time::sleep(poll_interval.min(remaining)).await;
            } else {
                // Event-driven: wait for a notification or the remaining
                // timeout, whichever comes first.
                tokio::select! {
                    _ = futures::future::select_all(long_poll_waiters.iter_mut()) => {
                        ::tracing::trace!(
                            user_id = %user_id,
                            "v2 sync long-poll woken by event notification"
                        );
                        // Loop back to re-register waiters and re-check DB.
                    }
                    _ = tokio::time::sleep(remaining) => {
                        return Ok(IncrementalUpdate::Timeout);
                    }
                }
            }
        }
    }

    async fn has_incremental_room_updates(&self, room_ids: &[String], since_stream_ord: i64) -> ApiResult<bool> {
        self.event_reader
            .has_room_events_since(room_ids, since_stream_ord)
            .await
            .map_err(map_internal!("Failed to poll for events"))
    }

    async fn has_incremental_to_device_updates(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        since_stream_id: i64,
    ) -> ApiResult<bool> {
        let Some(device_id) = device_id else {
            return Ok(false);
        };
        self.to_device_storage
            .has_messages_since(user_id, device_id, since_stream_id)
            .await
            .map_err(map_internal!("Failed to poll for to-device updates"))
    }

    async fn has_incremental_device_list_updates(&self, since_stream_id: i64) -> ApiResult<bool> {
        self.device_storage
            .has_device_list_updates_since(since_stream_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to poll for device-list updates", &e))
    }
}
