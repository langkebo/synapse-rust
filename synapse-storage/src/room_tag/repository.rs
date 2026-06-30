use async_trait::async_trait;
use std::sync::Arc;

use super::RoomTag;

#[async_trait]
pub trait RoomTagRepository: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn get_all_tags(&self, user_id: &str) -> Result<Vec<RoomTag>, sqlx::Error>;

    async fn get_tags(&self, user_id: &str, room_id: &str) -> Result<Vec<RoomTag>, sqlx::Error>;

    async fn add_tag(
        &self,
        user_id: &str,
        room_id: &str,
        tag: &str,
        order: Option<f64>,
    ) -> Result<(), sqlx::Error>;

    async fn remove_tag(&self, user_id: &str, room_id: &str, tag: &str) -> Result<(), sqlx::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_tag_repository_is_trait_object_safe() {
        fn _accept_trait_object(_: &dyn RoomTagRepository) {}
    }

    #[test]
    fn test_boxed_room_tag_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Box<dyn RoomTagRepository>>();
        assert_sync::<Box<dyn RoomTagRepository>>();
    }

    #[test]
    fn test_arced_room_tag_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<std::sync::Arc<dyn RoomTagRepository>>();
        assert_sync::<std::sync::Arc<dyn RoomTagRepository>>();
    }
}
