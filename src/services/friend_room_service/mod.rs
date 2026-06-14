pub use synapse_services::friend_room_service::{
    decode_friend_list_cursor, encode_friend_list_cursor, DirectMapUpdateAction, DirectRoomSnapshot, DmPartnerInfo,
    EnsureDirectRoomResult, FriendFederationSender, FriendListCursor, FriendListEntry, FriendListPage, FriendListRequest,
    FriendRoomCreateRoomConfig, FriendRoomRoomOps,
};

use crate::cache::CacheManager;
use crate::common::traits::FriendRoomProvider;
use crate::common::ApiError;
use crate::federation::friend::FriendFederationClient;
use crate::services::RoomService;
use crate::storage::{FriendRoomStorage, PresenceStorage, UserStorage};
use std::ops::Deref;
use std::sync::Arc;

pub struct FriendRoomService {
    inner: synapse_services::friend_room_service::FriendRoomService,
}

impl FriendRoomService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        friend_storage: FriendRoomStorage,
        room_service: Arc<RoomService>,
        user_storage: UserStorage,
        presence_storage: PresenceStorage,
        cache: &Arc<CacheManager>,
        server_name: String,
        key_rotation_manager: Arc<synapse_services::federation::KeyRotationManager>,
    ) -> Self {
        let federation_client = Arc::new(FriendFederationClient::new(server_name.clone(), Some(key_rotation_manager)));
        let inner = synapse_services::friend_room_service::FriendRoomService::new_with_dependencies(
            friend_storage,
            room_service,
            user_storage,
            presence_storage,
            Arc::new(cache.to_synapse_cache_manager()),
            server_name,
            federation_client,
        );

        Self { inner }
    }
}


impl Deref for FriendRoomService {
    type Target = synapse_services::friend_room_service::FriendRoomService;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[async_trait::async_trait]
impl FriendRoomProvider for FriendRoomService {
    async fn handle_incoming_friend_request(
        &self,
        user_id: &str,
        requester_id: &str,
        content: serde_json::Value,
    ) -> Result<(), ApiError> {
        self.inner.handle_incoming_friend_request(user_id, requester_id, content).await
    }
}
