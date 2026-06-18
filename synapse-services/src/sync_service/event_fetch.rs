use super::types::*;
use super::SyncService;
use crate::map_internal;
use crate::*;
use std::collections::HashMap;
use synapse_common::*;

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
            let since_stream_ordering = since_token
                .as_ref()
                .filter(|t| t.stream_id < Self::TIMESTAMP_TOKEN_MIN && t.stream_id > 0)
                .map(|t| t.stream_id);

            if let Some(stream_ord) = since_stream_ordering {
                let events = match event_filter.as_ref() {
                    Some(filter) => {
                        self.event_storage
                            .get_room_events_since_stream_batch_filtered(room_ids, stream_ord, fetch_limit, filter)
                            .await?
                    }
                    None => {
                        self.event_storage.get_room_events_since_stream_batch(room_ids, stream_ord, fetch_limit).await?
                    }
                };

                if events.values().all(|v| v.is_empty()) && timeout > 0 {
                    let update =
                        self.wait_for_incremental_update(user_id, device_id, room_ids, 0, since_token, timeout).await?;

                    match update {
                        IncrementalUpdate::Events => match event_filter.as_ref() {
                            Some(filter) => self
                                .event_storage
                                .get_room_events_since_stream_batch_filtered(room_ids, stream_ord, fetch_limit, filter)
                                .await
                                .map_err(Into::into),
                            None => self
                                .event_storage
                                .get_room_events_since_stream_batch(room_ids, stream_ord, fetch_limit)
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
                let since_ts = Self::event_since_ts(&since_token.map(|t| (*t).clone()));
                let events = match event_filter.as_ref() {
                    Some(filter) => {
                        self.event_storage
                            .get_room_events_since_batch_filtered(room_ids, since_ts, fetch_limit, filter)
                            .await?
                    }
                    None => self.event_storage.get_room_events_since_batch(room_ids, since_ts, fetch_limit).await?,
                };

                if events.values().all(|v| v.is_empty()) && timeout > 0 {
                    let update = self
                        .wait_for_incremental_update(user_id, device_id, room_ids, since_ts, since_token, timeout)
                        .await?;

                    match update {
                        IncrementalUpdate::Events => match event_filter.as_ref() {
                            Some(filter) => self
                                .event_storage
                                .get_room_events_since_batch_filtered(room_ids, since_ts, fetch_limit, filter)
                                .await
                                .map_err(Into::into),
                            None => self
                                .event_storage
                                .get_room_events_since_batch(room_ids, since_ts, fetch_limit)
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
            }
        } else {
            match event_filter.as_ref() {
                Some(filter) => self
                    .event_storage
                    .get_room_events_batch_filtered(room_ids, fetch_limit, filter)
                    .await
                    .map_err(Into::into),
                None => self.event_storage.get_room_events_batch(room_ids, fetch_limit).await.map_err(Into::into),
            }
        }
    }

    pub(crate) async fn wait_for_incremental_update(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        room_ids: &[String],
        since_ts: i64,
        since_token: Option<&SyncToken>,
        timeout: u64,
    ) -> ApiResult<IncrementalUpdate> {
        let timeout_duration = std::time::Duration::from_millis(timeout);
        let start = std::time::Instant::now();
        let poll_interval = self.sync_poll_interval();

        let since_to_device = since_token.and_then(|t| t.to_device_stream_id).unwrap_or(0);
        let since_device_lists = since_token.and_then(|t| t.device_list_stream_id).unwrap_or(0);

        loop {
            if start.elapsed() >= timeout_duration {
                return Ok(IncrementalUpdate::Timeout);
            }

            let (has_events, has_to_device, has_device_lists) = tokio::try_join!(
                self.has_incremental_room_updates(room_ids, since_ts),
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

            tokio::time::sleep(poll_interval).await;
        }
    }

    async fn has_incremental_room_updates(&self, room_ids: &[String], since_ts: i64) -> ApiResult<bool> {
        self.event_storage
            .has_room_events_since(room_ids, since_ts)
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
