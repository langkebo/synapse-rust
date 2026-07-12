use async_trait::async_trait;

use super::models::*;
use super::repository::FriendRoomStorage;

#[async_trait]
pub trait FriendRoomStoreApi: Send + Sync {
    async fn get_friend_list_room_id(&self, user_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_friend_list_content(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;
    async fn find_friend_lists_by_dm_room_id(&self, dm_room_id: &str) -> Result<Vec<FriendDmLink>, sqlx::Error>;
    async fn get_effective_direct_links_fallback(
        &self,
        user_id: &str,
    ) -> Result<Vec<DirectRoomFallbackLink>, sqlx::Error>;
    async fn get_existing_direct_room_id(&self, user_id: &str, friend_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_dm_partner_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<DmPartnerRecord>, sqlx::Error>;
    async fn get_friend_requests(
        &self,
        room_id: &str,
        request_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn is_friend(&self, room_id: &str, friend_id: &str) -> Result<bool, sqlx::Error>;
    async fn get_friend_info(&self, room_id: &str, friend_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;
    async fn get_friend_groups(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error>;
    async fn get_friend_groups_for_user(&self, room_id: &str, friend_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn create_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<(), sqlx::Error>;
    async fn delete_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<bool, sqlx::Error>;
    async fn rename_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn add_friend_to_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn remove_friend_from_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn create_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error>;
    async fn get_pending_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error>;
    async fn get_incoming_friend_requests(&self, receiver_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error>;
    async fn get_outgoing_friend_requests(&self, sender_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error>;
    async fn update_friend_request_status(
        &self,
        sender_id: &str,
        receiver_id: &str,
        status: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn delete_friend_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error>;
    async fn has_pending_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error>;
    async fn has_any_pending_request(&self, user_a: &str, user_b: &str) -> Result<bool, sqlx::Error>;
    async fn ensure_user_exists(&self, user_id: &str) -> Result<(), sqlx::Error>;
    async fn create_friend_request_with_user_ensure(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_mutual_friends(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_user_friend_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_shared_rooms(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn get_friend_suggestions_from_mutual_friends(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn get_friend_suggestions_from_shared_rooms(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;
}

#[async_trait]
impl FriendRoomStoreApi for FriendRoomStorage {
    async fn get_friend_list_room_id(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_friend_list_room_id(user_id).await
    }

    async fn get_friend_list_content(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_friend_list_content(room_id).await
    }

    async fn find_friend_lists_by_dm_room_id(&self, dm_room_id: &str) -> Result<Vec<FriendDmLink>, sqlx::Error> {
        self.find_friend_lists_by_dm_room_id(dm_room_id).await
    }

    async fn get_effective_direct_links_fallback(
        &self,
        user_id: &str,
    ) -> Result<Vec<DirectRoomFallbackLink>, sqlx::Error> {
        self.get_effective_direct_links_fallback(user_id).await
    }

    async fn get_existing_direct_room_id(&self, user_id: &str, friend_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.get_existing_direct_room_id(user_id, friend_id).await
    }

    async fn get_dm_partner_for_room(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<Option<DmPartnerRecord>, sqlx::Error> {
        self.get_dm_partner_for_room(room_id, user_id).await
    }

    async fn get_friend_requests(
        &self,
        room_id: &str,
        request_type: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_friend_requests(room_id, request_type).await
    }

    async fn is_friend(&self, room_id: &str, friend_id: &str) -> Result<bool, sqlx::Error> {
        self.is_friend(room_id, friend_id).await
    }

    async fn get_friend_info(&self, room_id: &str, friend_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_friend_info(room_id, friend_id).await
    }

    async fn get_friend_groups(&self, room_id: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
        self.get_friend_groups(room_id).await
    }

    async fn get_friend_groups_for_user(&self, room_id: &str, friend_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_friend_groups_for_user(room_id, friend_id).await
    }

    async fn create_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<(), sqlx::Error> {
        self.create_friend_group(room_id, user_id, group_name).await
    }

    async fn delete_friend_group(&self, room_id: &str, user_id: &str, group_name: &str) -> Result<bool, sqlx::Error> {
        self.delete_friend_group(room_id, user_id, group_name).await
    }

    async fn rename_friend_group(
        &self,
        room_id: &str,
        user_id: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<bool, sqlx::Error> {
        self.rename_friend_group(room_id, user_id, old_name, new_name).await
    }

    async fn add_friend_to_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.add_friend_to_group(room_id, user_id, group_name, friend_id).await
    }

    async fn remove_friend_from_group(
        &self,
        room_id: &str,
        user_id: &str,
        group_name: &str,
        friend_id: &str,
    ) -> Result<bool, sqlx::Error> {
        self.remove_friend_from_group(room_id, user_id, group_name, friend_id).await
    }

    async fn create_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        self.create_friend_request(sender_id, receiver_id, message).await
    }

    async fn get_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        self.get_friend_request(sender_id, receiver_id).await
    }

    async fn get_pending_friend_request(
        &self,
        sender_id: &str,
        receiver_id: &str,
    ) -> Result<Option<FriendRequestRecord>, sqlx::Error> {
        self.get_pending_friend_request(sender_id, receiver_id).await
    }

    async fn get_incoming_friend_requests(&self, receiver_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        self.get_incoming_friend_requests(receiver_id).await
    }

    async fn get_outgoing_friend_requests(&self, sender_id: &str) -> Result<Vec<FriendRequestRecord>, sqlx::Error> {
        self.get_outgoing_friend_requests(sender_id).await
    }

    async fn update_friend_request_status(
        &self,
        sender_id: &str,
        receiver_id: &str,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        self.update_friend_request_status(sender_id, receiver_id, status).await
    }

    async fn delete_friend_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error> {
        self.delete_friend_request(sender_id, receiver_id).await
    }

    async fn has_pending_request(&self, sender_id: &str, receiver_id: &str) -> Result<bool, sqlx::Error> {
        self.has_pending_request(sender_id, receiver_id).await
    }

    async fn has_any_pending_request(&self, user_a: &str, user_b: &str) -> Result<bool, sqlx::Error> {
        self.has_any_pending_request(user_a, user_b).await
    }

    async fn ensure_user_exists(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.ensure_user_exists(user_id).await
    }

    async fn create_friend_request_with_user_ensure(
        &self,
        sender_id: &str,
        receiver_id: &str,
        message: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        self.create_friend_request_with_user_ensure(sender_id, receiver_id, message).await
    }

    async fn get_mutual_friends(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_mutual_friends(user_id, target_user_id).await
    }

    async fn get_user_friend_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_user_friend_ids(user_id).await
    }

    async fn get_shared_rooms(&self, user_id: &str, target_user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        self.get_shared_rooms(user_id, target_user_id).await
    }

    async fn get_friend_suggestions_from_mutual_friends(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_friend_suggestions_from_mutual_friends(user_id, limit).await
    }

    async fn get_friend_suggestions_from_shared_rooms(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_friend_suggestions_from_shared_rooms(user_id, limit).await
    }
}
