//! Room summary service — core CRUD operations.
//!
//! State and sync operations live in [`summary_state`].
//! Stats and queue operations live in [`summary_stats`].

use crate::common::{ApiError, ApiResult};
use crate::storage::event::EventStorage;
use crate::storage::membership::RoomMemberStorage;
use crate::storage::room_summary::*;
pub use crate::storage::room_summary::{
    CreateRoomSummaryRequest, CreateSummaryMemberRequest, RoomSummaryMember, RoomSummaryResponse, RoomSummaryState,
    RoomSummaryStats, UpdateRoomSummaryRequest, UpdateSummaryMemberRequest,
};
use std::sync::Arc;
use tracing::{debug, info, instrument};

pub struct RoomSummaryService {
    pub(crate) storage: Arc<RoomSummaryStorage>,
    pub(crate) event_storage: Arc<EventStorage>,
    pub(crate) member_storage: Option<Arc<RoomMemberStorage>>,
}

impl RoomSummaryService {
    pub fn new(
        storage: Arc<RoomSummaryStorage>,
        event_storage: Arc<EventStorage>,
        member_storage: Option<Arc<RoomMemberStorage>>,
    ) -> Self {
        Self { storage, event_storage, member_storage }
    }

    #[instrument(skip(self))]
    pub async fn get_summary(&self, room_id: &str) -> Result<Option<RoomSummaryResponse>, ApiError> {
        let summary_res = self.storage.get_summary(room_id).await;

        let summary = match summary_res {
            Ok(s) => s,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get room summary", &e)),
        };

        if let Some(summary) = summary {
            let heroes = self.get_heroes(room_id).await?;
            Ok(Some(summary.to_response(heroes)))
        } else {
            Ok(None)
        }
    }

    #[instrument(skip(self))]
    pub async fn get_summaries_for_user(&self, user_id: &str) -> Result<Vec<RoomSummaryResponse>, ApiError> {
        let summaries_res = self.storage.get_summaries_for_user(user_id).await;

        let summaries = match summaries_res {
            Ok(s) => s,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get user room summaries", &e)),
        };

        if summaries.is_empty() {
            return Ok(Vec::new());
        }

        let room_ids: Vec<String> = summaries.iter().map(|s| s.room_id.clone()).collect();
        let heroes_map = self.get_heroes_batch(&room_ids).await?;

        let responses = summaries
            .into_iter()
            .map(|summary| {
                let heroes = heroes_map.get(&summary.room_id).cloned().unwrap_or_default();
                summary.to_response(heroes)
            })
            .collect();

        Ok(responses)
    }

    pub(crate) async fn get_heroes(&self, room_id: &str) -> Result<Vec<RoomSummaryHero>, ApiError> {
        let members_res = self.storage.get_heroes(room_id, 5).await;

        let members = match members_res {
            Ok(m) => m,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get heroes", &e)),
        };

        Ok(members.into_iter().map(RoomSummaryHero::from).collect())
    }

    /// Batch variant of [`get_heroes`] that fetches heroes for multiple rooms
    /// in a single query, returning heroes keyed by `room_id`.
    pub(crate) async fn get_heroes_batch(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<RoomSummaryHero>>, ApiError> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let members_res = self.storage.get_heroes_batch(room_ids, 5).await;

        let members_map = match members_res {
            Ok(m) => m,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get heroes batch", &e)),
        };

        Ok(members_map
            .into_iter()
            .map(|(room_id, members)| (room_id, members.into_iter().map(RoomSummaryHero::from).collect()))
            .collect())
    }

    pub async fn create_summary(&self, request: CreateRoomSummaryRequest) -> ApiResult<RoomSummaryResponse> {
        info!(room_id = %request.room_id, "Creating room summary");

        let room_id = request.room_id.clone();

        let summary_exists_res = self.storage.get_summary(&room_id).await;

        let exists = match summary_exists_res {
            Ok(s) => s.is_some(),
            Err(e) => return Err(ApiError::internal_with_log("Failed to check room summary", &e)),
        };

        if exists {
            let update_res =
                self.storage.update_summary(&room_id, Self::create_request_to_update_request(&request)).await;
            if let Err(e) = update_res {
                return Err(ApiError::internal_with_log("Failed to update room summary", &e));
            }
        } else {
            let create_res = self.storage.create_summary(request).await;
            if let Err(e) = create_res {
                return Err(ApiError::internal_with_log("Failed to create room summary", &e));
            }
        }

        self.synchronize_room_snapshot(&room_id).await?;

        let final_summary: Option<RoomSummaryResponse> = match self.get_summary(&room_id).await {
            Ok(s) => s,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get summary after sync", &e)),
        };
        final_summary.ok_or_else(|| ApiError::not_found("Room summary not found after sync"))
    }

    fn create_request_to_update_request(request: &CreateRoomSummaryRequest) -> UpdateRoomSummaryRequest {
        UpdateRoomSummaryRequest {
            name: request.name.clone(),
            topic: request.topic.clone(),
            avatar_url: request.avatar_url.clone(),
            canonical_alias: request.canonical_alias.clone(),
            join_rule: request.join_rule.clone(),
            history_visibility: request.history_visibility.clone(),
            guest_access: request.guest_access.clone(),
            is_direct: request.is_direct,
            is_space: request.is_space,
            ..Default::default()
        }
    }

    #[instrument(skip(self))]
    pub async fn update_summary(
        &self,
        room_id: &str,
        request: UpdateRoomSummaryRequest,
    ) -> Result<RoomSummaryResponse, ApiError> {
        let summary = self
            .storage
            .update_summary(room_id, request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update room summary", &e))?;

        let heroes = self.get_heroes(room_id).await?;
        Ok(summary.to_response(heroes))
    }

    #[instrument(skip(self))]
    pub async fn delete_summary(&self, room_id: &str) -> Result<(), ApiError> {
        info!(room_id = %room_id, "Deleting room summary");

        self.storage
            .delete_summary(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete room summary", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn add_member(&self, request: CreateSummaryMemberRequest) -> Result<RoomSummaryMember, ApiError> {
        debug!("Adding member {} to room {}", request.user_id, request.room_id);

        let member = self
            .storage
            .add_member(request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to add member", &e))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn update_member(
        &self,
        room_id: &str,
        user_id: &str,
        request: UpdateSummaryMemberRequest,
    ) -> Result<RoomSummaryMember, ApiError> {
        let member = self
            .storage
            .update_member(room_id, user_id, request)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update member", &e))?;

        Ok(member)
    }

    #[instrument(skip(self))]
    pub async fn remove_member(&self, room_id: &str, user_id: &str) -> Result<(), ApiError> {
        debug!("Removing member {} from room {}", user_id, room_id);

        self.storage
            .remove_member(room_id, user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to remove member", &e))?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_members(&self, room_id: &str) -> Result<Vec<RoomSummaryMember>, ApiError> {
        let members = self
            .storage
            .get_members(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get members", &e))?;

        Ok(members)
    }

    #[instrument(skip(self))]
    pub async fn get_summaries_by_ids(&self, room_ids: &[String]) -> Result<Vec<RoomSummaryResponse>, ApiError> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let summaries_res = self.storage.get_summaries_by_ids(room_ids).await;

        let summaries = match summaries_res {
            Ok(s) => s,
            Err(e) => return Err(ApiError::internal_with_log("Failed to get room summaries", &e)),
        };

        if summaries.is_empty() {
            return Ok(Vec::new());
        }

        let fetched_room_ids: Vec<String> = summaries.iter().map(|s| s.room_id.clone()).collect();
        let heroes_map = self.get_heroes_batch(&fetched_room_ids).await?;

        let responses = summaries
            .into_iter()
            .map(|summary| {
                let heroes = heroes_map.get(&summary.room_id).cloned().unwrap_or_default();
                summary.to_response(heroes)
            })
            .collect();

        Ok(responses)
    }
}
