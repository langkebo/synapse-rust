use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use synapse_common::ApiResult;
use synapse_federation::client_api::FederationClientApi;

use super::backfill::BackfillOutcome;
use super::lifecycle::service::LifecycleService;
use super::membership::service::MembershipService;
use super::messaging::service::MessagingService;
use super::service::RoomService;
use super::state::service::RoomStateService;
use crate::room_summary_service::RoomSummaryService;

#[async_trait]
pub trait RoomServiceApi: Send + Sync {
    async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value>;

    async fn get_room_state(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value>;

    async fn get_user_rooms(&self, user_id: &str) -> ApiResult<serde_json::Value>;

    async fn collect_child_rooms(&self, child_room_ids: &[String]) -> ApiResult<Vec<Value>>;

    async fn upgrade_room(&self, old_room_id: &str, new_version: &str, user_id: &str) -> ApiResult<String>;

    async fn dispatch_appservice_event(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    );

    async fn backfill_room_history(
        &self,
        federation_client: &Arc<dyn FederationClientApi>,
        room_id: &str,
        limit: Option<u32>,
    ) -> ApiResult<BackfillOutcome>;

    fn membership(&self) -> &MembershipService;

    fn messaging(&self) -> &MessagingService;

    fn state(&self) -> &RoomStateService;

    fn lifecycle(&self) -> &LifecycleService;

    fn room_summary_service(&self) -> &RoomSummaryService;
}

#[async_trait]
impl RoomServiceApi for RoomService {
    async fn get_room(&self, room_id: &str) -> ApiResult<serde_json::Value> {
        self.get_room(room_id).await
    }

    async fn get_room_state(&self, room_id: &str, user_id: &str) -> ApiResult<serde_json::Value> {
        self.get_room_state(room_id, user_id).await
    }

    async fn get_user_rooms(&self, user_id: &str) -> ApiResult<serde_json::Value> {
        self.get_user_rooms(user_id).await
    }

    async fn collect_child_rooms(&self, child_room_ids: &[String]) -> ApiResult<Vec<Value>> {
        self.collect_child_rooms(child_room_ids).await
    }

    async fn upgrade_room(&self, old_room_id: &str, new_version: &str, user_id: &str) -> ApiResult<String> {
        self.upgrade_room(old_room_id, new_version, user_id).await
    }

    async fn dispatch_appservice_event(
        &self,
        event_id: &str,
        room_id: &str,
        event_type: &str,
        sender: &str,
        content: &serde_json::Value,
        state_key: Option<&str>,
    ) {
        self.dispatch_appservice_event(event_id, room_id, event_type, sender, content, state_key).await
    }

    async fn backfill_room_history(
        &self,
        federation_client: &Arc<dyn FederationClientApi>,
        room_id: &str,
        limit: Option<u32>,
    ) -> ApiResult<BackfillOutcome> {
        self.backfill_room_history(federation_client, room_id, limit).await
    }

    fn membership(&self) -> &MembershipService {
        &self.membership
    }

    fn messaging(&self) -> &MessagingService {
        &self.messaging
    }

    fn state(&self) -> &RoomStateService {
        &self.state
    }

    fn lifecycle(&self) -> &LifecycleService {
        &self.lifecycle
    }

    fn room_summary_service(&self) -> &RoomSummaryService {
        &self.room_summary_service
    }
}
