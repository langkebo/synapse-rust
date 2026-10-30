//! The account-scoped cleanup half of the deactivation pipeline.
//!
//! Split out of [`super`] purely for file size: the purge is one seam
//! ([`UserDataPurger`]) over a dozen storage handles, and it has no ordering
//! relationship with the pipeline that calls it. Everything here is re-exported
//! from the parent module, so `crate::account_deactivation_service::*` and the
//! integration tests keep their paths.

use std::sync::Arc;

use sqlx::PgPool;
use synapse_cache::CacheManager;
use synapse_common::{ApiError, ApiResult};
use synapse_e2ee::backup::storage::KeyBackupStorage;
use synapse_e2ee::secure_backup::SecureBackupService;
use synapse_storage::account_data::AccountDataStorage;
use synapse_storage::presence::PresenceStorage;
use synapse_storage::push::PushStorage;
use synapse_storage::room::RoomStorage;
use synapse_storage::room_account_data::RoomAccountDataStorage;
use synapse_storage::room_tag::RoomTagStorage;
use synapse_storage::thread::ThreadStorage;
use synapse_storage::threepid::ThreepidStorage;

/// Rows removed by [`UserDataPurger::purge`], one count per table.
///
/// Split out (rather than summed) so an operator reading the audit log can tell
/// *which* store still held data for an account they thought they had deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgedUserData {
    /// `account_data` rows.
    pub account_data_deleted: u64,
    /// `room_account_data` rows.
    pub room_account_data_deleted: u64,
    /// `push_rules` rows.
    pub push_rules_deleted: u64,
    /// `thread_subscriptions` rows.
    pub thread_subscriptions_deleted: u64,
    /// `user_threepids` rows.
    pub threepids_removed: u64,
    /// `key_backups` rows (their `backup_keys` cascade away with them).
    pub key_backups_deleted: u64,
    /// `secure_key_backups` rows (their session keys cascade away with them).
    pub secure_backups_deleted: u64,
    /// `room_tags` rows (per-room favourites / low-priority ordering).
    pub room_tags_deleted: u64,
    /// `event_receipts` rows (current read-receipt state).
    pub event_receipts_deleted: u64,
    /// `event_receipts_stream` rows (the MSC2409 receipt log).
    pub event_receipts_stream_deleted: u64,
    /// `thread_read_receipts` rows (per-thread unread counters).
    pub thread_read_receipts_deleted: u64,
    /// `presence` rows (last known presence / status message).
    pub presence_deleted: u64,
}

/// Every table [`StorageUserDataPurger`] deletes from, in purge order.
///
/// # Why a constant and not just five more `DELETE`s
///
/// The `PurgedUserData` counts say *how many* rows went; they cannot say whether
/// a table was forgotten entirely — a table nobody deletes from reports `0`
/// forever and looks identical to "this account had none". This list is the
/// assertion subject for
/// `tests/integration/account_deactivation_purge_tests.rs`, which checks two
/// things no count can: every entry exists in the migration baseline and carries
/// a `user_id` column (so the `DELETE … WHERE user_id = $1` is well-formed), and
/// every entry is actually emptied by a real purge run.
///
/// It is also the honest answer to "which tables survive deactivation?" — the
/// `users` row itself deliberately does (deactivation is `deactivated = TRUE`,
/// not a row delete), so anything reachable only through `ON DELETE CASCADE` on
/// `users` is *not* covered by the cascade and has to be listed here.
pub const ACCOUNT_DEACTIVATION_PURGED_TABLES: &[&str] = &[
    "account_data",
    "room_account_data",
    "push_rules",
    "thread_subscriptions",
    "user_threepids",
    "key_backups",
    "secure_key_backups",
    "room_tags",
    "event_receipts",
    "event_receipts_stream",
    "thread_read_receipts",
    "presence",
];

/// The account-scoped cleanup half of the pipeline.
///
/// A seam for the same reason [`super::RoomParting`] is one: it lets the
/// *pipeline* (order, error policy, "irreversible step last") be tested offline
/// with a fake, while production uses [`StorageUserDataPurger`] over the real
/// stores.
#[async_trait::async_trait]
pub trait UserDataPurger: Send + Sync {
    /// Remove every account-scoped row owned by `user_id` that deactivation is
    /// responsible for. See [`PurgedUserData`] for the tables.
    async fn purge(&self, user_id: &str) -> ApiResult<PurgedUserData>;
}

/// Production [`UserDataPurger`]: the storage handles for every table in
/// [`ACCOUNT_DEACTIVATION_PURGED_TABLES`], all built from one pool.
///
/// Composing them here (rather than taking a dozen `Arc<dyn …>` constructor
/// arguments) keeps the container wiring to two arguments and keeps the service's
/// dependency list — the tests' fake surface — at one seam.
pub struct StorageUserDataPurger {
    account_data: AccountDataStorage,
    room_account_data: RoomAccountDataStorage,
    push_storage: PushStorage,
    thread_storage: ThreadStorage,
    threepid_storage: ThreepidStorage,
    key_backup_storage: KeyBackupStorage,
    secure_backup: SecureBackupService,
    room_tag_storage: RoomTagStorage,
    room_storage: RoomStorage,
    presence: PresenceStorage,
}

impl StorageUserDataPurger {
    /// Build every store from `pool`. Each storage's constructor takes the pool
    /// by one of three flavors (`&Arc`, `Arc`, `&`), so they are all fed from the
    /// same handle.
    ///
    /// `cache` is the process-wide [`CacheManager`], **not** a fresh instance:
    /// [`PresenceStorage::delete_all_presence_for_user`] invalidates
    /// [`synapse_cache::CacheKeyBuilder::user_presence`], and an invalidation
    /// against a private cache would leave the deactivated account showing as
    /// "online" to subscribers until the TTL expired.
    pub fn new(pool: &Arc<PgPool>, cache: Arc<CacheManager>) -> Self {
        Self {
            account_data: AccountDataStorage::new(pool),
            room_account_data: RoomAccountDataStorage::new(pool),
            push_storage: PushStorage::new(pool.clone()),
            thread_storage: ThreadStorage::new(pool),
            threepid_storage: ThreepidStorage::new(pool.as_ref()),
            key_backup_storage: KeyBackupStorage::new(pool),
            secure_backup: SecureBackupService::new(pool),
            room_tag_storage: RoomTagStorage::new(pool.clone()),
            room_storage: RoomStorage::new(pool),
            presence: PresenceStorage::new(pool.clone(), cache),
        }
    }
}

#[async_trait::async_trait]
impl UserDataPurger for StorageUserDataPurger {
    async fn purge(&self, user_id: &str) -> ApiResult<PurgedUserData> {
        // The receipts pair is one transaction in the store, so it is fetched
        // together and split into the two per-table counts here.
        let (event_receipts_deleted, event_receipts_stream_deleted) =
            self.room_storage.delete_all_receipts_for_user(user_id).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to delete receipts during account deactivation", e)
            })?;

        Ok(PurgedUserData {
            account_data_deleted: self.account_data.delete_all_account_data_for_user(user_id).await?,
            room_account_data_deleted: self.room_account_data.delete_all_room_account_data_for_user(user_id).await?,
            push_rules_deleted: self.push_storage.delete_all_push_rules_for_user(user_id).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to delete push rules during account deactivation", e)
            })?,
            thread_subscriptions_deleted: self
                .thread_storage
                .delete_all_subscriptions_for_user(user_id)
                .await
                .map_err(|e| {
                    ApiError::internal_with_cause(
                        "Failed to delete thread subscriptions during account deactivation",
                        e,
                    )
                })?,
            threepids_removed: self.threepid_storage.remove_threepids_by_user(user_id).await?,
            key_backups_deleted: self.key_backup_storage.delete_all_for_user(user_id).await?,
            secure_backups_deleted: self.secure_backup.delete_all_for_user(user_id).await?,
            room_tags_deleted: self.room_tag_storage.delete_all_tags_for_user(user_id).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to delete room tags during account deactivation", e)
            })?,
            event_receipts_deleted,
            event_receipts_stream_deleted,
            thread_read_receipts_deleted: self
                .thread_storage
                .delete_all_read_receipts_for_user(user_id)
                .await
                .map_err(|e| {
                    ApiError::internal_with_cause(
                        "Failed to delete thread read receipts during account deactivation",
                        e,
                    )
                })?,
            presence_deleted: self.presence.delete_all_presence_for_user(user_id).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to delete presence during account deactivation", e)
            })?,
        })
    }
}
