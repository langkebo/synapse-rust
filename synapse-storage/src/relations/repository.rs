use async_trait::async_trait;
use std::sync::Arc;

use super::{AggregationResult, CreateRelationParams, EventRelation, RelationQueryParams};

#[async_trait]
pub trait RelationsRepository: Send + Sync {
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn create_relation(&self, params: CreateRelationParams) -> Result<EventRelation, sqlx::Error>;

    async fn get_relation(&self, room_id: &str, event_id: &str) -> Result<Option<EventRelation>, sqlx::Error>;

    async fn count_relations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: Option<&str>,
    ) -> Result<i64, sqlx::Error>;

    async fn get_relations(&self, params: RelationQueryParams) -> Result<Vec<EventRelation>, sqlx::Error>;

    async fn get_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<EventRelation>, sqlx::Error>;

    async fn get_references(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        limit: Option<i32>,
    ) -> Result<Vec<EventRelation>, sqlx::Error>;

    async fn get_replacement(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        sender: &str,
    ) -> Result<Option<EventRelation>, sqlx::Error>;

    async fn aggregate_annotations(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
    ) -> Result<Vec<AggregationResult>, sqlx::Error>;

    async fn redact_relation(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error>;

    async fn delete_relation(&self, room_id: &str, event_id: &str, sender: &str) -> Result<bool, sqlx::Error>;

    async fn relation_exists(
        &self,
        room_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        sender: &str,
    ) -> Result<bool, sqlx::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relations_repository_is_trait_object_safe() {
        fn _accept_trait_object(_: &dyn RelationsRepository) {}
    }

    #[test]
    fn test_boxed_relations_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Box<dyn RelationsRepository>>();
        assert_sync::<Box<dyn RelationsRepository>>();
    }

    #[test]
    fn test_arced_relations_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<std::sync::Arc<dyn RelationsRepository>>();
        assert_sync::<std::sync::Arc<dyn RelationsRepository>>();
    }
}
