use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use super::PresenceSnapshot;

#[async_trait]
pub trait PresenceRepository: Send + Sync {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn set_presence(
        &self,
        user_id: &str,
        presence: &str,
        status_msg: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn get_presence(&self, user_id: &str) -> Result<Option<(String, Option<String>)>, sqlx::Error>;

    async fn get_presence_with_meta(
        &self,
        user_id: &str,
    ) -> Result<Option<(String, Option<String>, Option<i64>)>, sqlx::Error>;

    async fn get_presences(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, (String, Option<String>)>, sqlx::Error>;

    async fn set_typing(&self, room_id: &str, user_id: &str, typing: bool) -> Result<(), sqlx::Error>;

    async fn add_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error>;

    async fn remove_subscription(&self, subscriber_id: &str, target_id: &str) -> Result<(), sqlx::Error>;

    async fn get_subscriptions(&self, subscriber_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn get_subscribers(&self, target_id: &str) -> Result<Vec<String>, sqlx::Error>;

    async fn get_presence_batch(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>)>, sqlx::Error>;

    async fn get_presence_batch_with_meta(
        &self,
        user_ids: &[String],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error>;

    async fn get_presence_snapshots(
        &self,
        user_ids: &[String],
    ) -> Result<HashMap<String, PresenceSnapshot>, sqlx::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time verification that `PresenceRepository` is object-safe and can
    /// be used as `Box<dyn PresenceRepository>`. The trait bound `Send + Sync`
    /// declared on the trait itself must propagate to the trait object so that
    /// `Box<dyn PresenceRepository>` is also `Send + Sync` (required for storage
    /// in `Arc<dyn PresenceRepository>` / `ServiceContainer`).
    #[test]
    fn test_presence_repository_is_trait_object_safe() {
        fn _accept_trait_object(_: &dyn PresenceRepository) {}
        // No runtime assertions — this test passes as long as the trait can be
        // referenced as a trait object (compile-time check).
    }

    /// Compile-time verification that `Box<dyn PresenceRepository>` is `Send + Sync`.
    /// The trait declares `: Send + Sync`, so any `dyn PresenceRepository` is also
    /// `Send + Sync`. This test fails to compile if the trait bounds are
    /// accidentally removed.
    #[test]
    fn test_boxed_presence_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<Box<dyn PresenceRepository>>();
        assert_sync::<Box<dyn PresenceRepository>>();
    }

    /// Compile-time verification that `Arc<dyn PresenceRepository>` is `Send + Sync`.
    /// `Arc<dyn PresenceRepository>` is the canonical storage shape used by
    /// `ServiceContainer`; if the trait loses `Send + Sync` the entire service
    /// container would fail to compile.
    #[test]
    fn test_arced_presence_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<std::sync::Arc<dyn PresenceRepository>>();
        assert_sync::<std::sync::Arc<dyn PresenceRepository>>();
    }

    /// Compile-time verification that the trait can be referenced from a
    /// sibling module path. The `use super::*` import should expose
    /// `PresenceRepository` to the test module.
    #[test]
    fn test_presence_repository_in_scope() {
        // This test verifies the `PresenceRepository` trait is reachable from the
        // test module via `use super::*`. If the trait is renamed or made
        // private, this test will fail to compile.
        let _ = std::any::TypeId::of::<dyn PresenceRepository>();
    }
}
