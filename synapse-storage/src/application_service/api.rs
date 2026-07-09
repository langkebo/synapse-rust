use async_trait::async_trait;

use super::models::*;
use super::repository::ApplicationServiceStorage;

#[async_trait]
pub trait ApplicationServiceStoreApi: Send + Sync {
    async fn register(&self, request: RegisterApplicationServiceRequest) -> Result<ApplicationService, sqlx::Error>;
    async fn upsert_registration(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error>;
    async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error>;
    async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error>;
    async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error>;
    async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error>;
    async fn update(
        &self,
        as_id: &str,
        request: &UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error>;
    async fn update_timestamp(&self, as_id: &str) -> Result<(), sqlx::Error>;
    async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error>;
    async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, sqlx::Error>;
    async fn get_state(&self, as_id: &str, state_key: &str) -> Result<Option<ApplicationServiceState>, sqlx::Error>;
    async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error>;
    #[allow(clippy::too_many_arguments)]
    async fn add_event(
        &self,
        event_id: &str,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        _sender: &str,
        _content: serde_json::Value,
        _state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, sqlx::Error>;
    async fn get_pending_events(&self, as_id: &str, limit: i64) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error>;
    async fn count_pending_events(&self, as_id: &str) -> Result<i64, sqlx::Error>;
    async fn mark_event_processed(&self, event_id: &str) -> Result<(), sqlx::Error>;
    async fn create_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<ApplicationServiceTransaction, sqlx::Error>;
    async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error>;
    async fn fail_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        error: &str,
    ) -> Result<ApplicationServiceTransaction, sqlx::Error>;
    async fn get_pending_transactions(&self, as_id: &str) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error>;
    async fn count_pending_transactions(&self, as_id: &str) -> Result<i64, sqlx::Error>;
    async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, sqlx::Error>;
    async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, sqlx::Error>;
    async fn has_exclusive_user_namespace_match(&self, as_id: &str, user_id: &str) -> Result<bool, sqlx::Error>;
    async fn find_user_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn find_room_alias_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn find_room_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error>;
    async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error>;
    async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error>;
    async fn get_room_alias_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error>;
    async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error>;
    async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error>;
    async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error>;
}

#[async_trait]
impl ApplicationServiceStoreApi for ApplicationServiceStorage {
    async fn register(&self, request: RegisterApplicationServiceRequest) -> Result<ApplicationService, sqlx::Error> {
        self.register(request).await
    }

    async fn upsert_registration(
        &self,
        request: RegisterApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        self.upsert_registration(request).await
    }

    async fn get_by_id(&self, as_id: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        self.get_by_id(as_id).await
    }

    async fn get_by_token(&self, as_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        self.get_by_token(as_token).await
    }

    async fn get_by_hs_token(&self, hs_token: &str) -> Result<Option<ApplicationService>, sqlx::Error> {
        self.get_by_hs_token(hs_token).await
    }

    async fn get_all_active(&self) -> Result<Vec<ApplicationService>, sqlx::Error> {
        self.get_all_active().await
    }

    async fn update(
        &self,
        as_id: &str,
        request: &UpdateApplicationServiceRequest,
    ) -> Result<ApplicationService, sqlx::Error> {
        self.update(as_id, request).await
    }

    async fn update_timestamp(&self, as_id: &str) -> Result<(), sqlx::Error> {
        self.update_timestamp(as_id).await
    }

    async fn unregister(&self, as_id: &str) -> Result<(), sqlx::Error> {
        self.unregister(as_id).await
    }

    async fn set_state(
        &self,
        as_id: &str,
        state_key: &str,
        state_value: &str,
    ) -> Result<ApplicationServiceState, sqlx::Error> {
        self.set_state(as_id, state_key, state_value).await
    }

    async fn get_state(&self, as_id: &str, state_key: &str) -> Result<Option<ApplicationServiceState>, sqlx::Error> {
        self.get_state(as_id, state_key).await
    }

    async fn get_all_states(&self, as_id: &str) -> Result<Vec<ApplicationServiceState>, sqlx::Error> {
        self.get_all_states(as_id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn add_event(
        &self,
        event_id: &str,
        as_id: &str,
        room_id: &str,
        event_type: &str,
        _sender: &str,
        _content: serde_json::Value,
        _state_key: Option<&str>,
    ) -> Result<ApplicationServiceEvent, sqlx::Error> {
        self.add_event(event_id, as_id, room_id, event_type, _sender, _content, _state_key).await
    }

    async fn get_pending_events(&self, as_id: &str, limit: i64) -> Result<Vec<ApplicationServiceEvent>, sqlx::Error> {
        self.get_pending_events(as_id, limit).await
    }

    async fn count_pending_events(&self, as_id: &str) -> Result<i64, sqlx::Error> {
        self.count_pending_events(as_id).await
    }

    async fn mark_event_processed(&self, event_id: &str) -> Result<(), sqlx::Error> {
        self.mark_event_processed(event_id).await
    }

    async fn create_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<ApplicationServiceTransaction, sqlx::Error> {
        self.create_transaction(as_id, transaction_id, events).await
    }

    async fn complete_transaction(&self, as_id: &str, transaction_id: &str) -> Result<(), sqlx::Error> {
        self.complete_transaction(as_id, transaction_id).await
    }

    async fn fail_transaction(
        &self,
        as_id: &str,
        transaction_id: &str,
        error: &str,
    ) -> Result<ApplicationServiceTransaction, sqlx::Error> {
        self.fail_transaction(as_id, transaction_id, error).await
    }

    async fn get_pending_transactions(&self, as_id: &str) -> Result<Vec<ApplicationServiceTransaction>, sqlx::Error> {
        self.get_pending_transactions(as_id).await
    }

    async fn count_pending_transactions(&self, as_id: &str) -> Result<i64, sqlx::Error> {
        self.count_pending_transactions(as_id).await
    }

    async fn register_virtual_user(
        &self,
        as_id: &str,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<ApplicationServiceUser, sqlx::Error> {
        self.register_virtual_user(as_id, user_id, displayname, avatar_url).await
    }

    async fn get_virtual_users(&self, as_id: &str) -> Result<Vec<ApplicationServiceUser>, sqlx::Error> {
        self.get_virtual_users(as_id).await
    }

    async fn has_exclusive_user_namespace_match(&self, as_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        self.has_exclusive_user_namespace_match(as_id, user_id).await
    }

    async fn find_user_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.find_user_namespace_conflict(as_id, namespace_pattern).await
    }

    async fn find_room_alias_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.find_room_alias_namespace_conflict(as_id, namespace_pattern).await
    }

    async fn find_room_namespace_conflict(
        &self,
        as_id: &str,
        namespace_pattern: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        self.find_room_namespace_conflict(as_id, namespace_pattern).await
    }

    async fn is_user_in_namespace(&self, user_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.is_user_in_namespace(user_id).await
    }

    async fn is_room_alias_in_namespace(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        self.is_room_alias_in_namespace(alias).await
    }

    async fn is_room_id_in_namespace(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.is_room_id_in_namespace(room_id).await
    }

    async fn get_user_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        self.get_user_namespaces(as_id).await
    }

    async fn get_room_alias_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        self.get_room_alias_namespaces(as_id).await
    }

    async fn get_room_namespaces(&self, as_id: &str) -> Result<Vec<ApplicationServiceNamespace>, sqlx::Error> {
        self.get_room_namespaces(as_id).await
    }

    async fn get_statistics(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        self.get_statistics().await
    }

    async fn update_last_seen(&self, as_id: &str) -> Result<(), sqlx::Error> {
        self.update_last_seen(as_id).await
    }
}
