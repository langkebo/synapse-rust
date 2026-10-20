//! Account deactivation orchestration — parity with upstream Synapse's
//! `synapse/handlers/deactivate_account.py`.
//!
//! # Why this is a service of its own
//!
//! Deactivating an account is a **pipeline**, not a flag flip. Upstream
//! v1.163.0 does, in order: cancel the user's MSC4140 delayed events (explicitly
//! "first, so that none of them get sent while the rest of the deactivation is in
//! progress"), unbind/delete threepids, delete devices + access tokens, delete
//! **all** pushers, mark the account deactivated, then part the user from their
//! rooms.
//!
//! This repo used to do only the middle of that: `deactivate_user` sets
//! `is_deactivated` and revokes tokens/devices, and nothing else. The
//! consequences were real, not cosmetic:
//!
//! * a message the user had scheduled with MSC4140 was **still delivered** after
//!   the account was deactivated;
//! * the user stayed a `join` member of every room (so member lists, power-level
//!   arithmetic and room summaries kept counting them);
//! * pushers survived — `pushers` has no foreign key to `devices`, so deleting
//!   the devices does not remove them;
//! * the user's threepids, account data, room account data, push rules, thread
//!   subscriptions and E2EE backups all survived, so a "deactivated" account
//!   still had addresses and key material attached to it;
//! * outstanding **invites and knocks** were left standing: the user was gone
//!   but still reachable through a pending membership nobody had rejected.
//!
//! # Why it is not a method on `RegistrationService`
//!
//! `RegistrationService` is built in the *core* phase of
//! [`crate::container::ServiceContainer`], before the room and admin groups
//! exist; the pipeline needs `RoomService` (membership) and
//! `DelayedEventService` (MSC4140). This service is therefore assembled in the
//! container's phase 4, where all of them are in scope.
//!
//! # Ordering (and why parting is asynchronous)
//!
//! Upstream marks the account deactivated **without** parting it from its
//! joined rooms inline: it writes a `user_pending_deactivations` row and starts
//! a background parting loop, so a user in thousands of rooms cannot block the
//! request and an unfinished parting survives a restart. This repo follows the
//! same model:
//!
//! 1. cancel delayed events (**fail-fast**, security-critical);
//! 2. reject every pending invite/knock (**best-effort per room**, fail-fast on
//!    listing) — kept synchronous because a pending membership still makes the
//!    account addressable, and it must not outlive the response;
//! 3. enqueue the user for background parting of **joined** rooms
//!    (**fail-fast**: if the row cannot be written, nothing will ever part the
//!    user, and the teardown below is irreversible);
//! 4. delete all pushers (**fail-fast**);
//! 5. purge the account-scoped rows — account data, room account data, push
//!    rules, thread subscriptions, threepids and E2EE backups (**fail-fast**);
//! 6. revoke tokens/devices and mark deactivated (**fail-fast, last**), then
//!    wake the background parting loop without awaiting it.
//!
//! Putting the irreversible step last means a retry after a partial failure is
//! meaningful: the user can press "deactivate" again and the pipeline re-runs
//! from the top. Doing it first would leave an account that is deactivated but
//! still in rooms, with no mechanism to finish the job.
//!
//! [`AccountDeactivationService::process_pending_parts`] is the drain loop
//! (upstream's `_user_parter_loop`); the server runs it at startup (restart
//! recovery) and on every maintenance tick.
//!
//! # Rejecting pending invites/knocks
//!
//! Step 2 targets rooms with a pending membership (`invite` / `knock`). A
//! pending membership is not a `join`, so the old pipeline skipped it and the
//! invitee stayed reachable; driving `leave` on it is exactly upstream's
//! `reject_invite`/`retract_knock` (see
//! [`synapse_common::membership_transition`]: a self-`leave` from `invite`/`knock`
//! is legal), so no new transition had to be invented.
//!
//! # What "purge" does **not** touch
//!
//! The `user_directory` table has no Rust reader or writer anywhere in this repo
//! — it is dead schema. The user-visible behaviour upstream gets by deleting a
//! `user_directory` row ("this user no longer turns up in a directory search")
//! is already provided by the `is_deactivated` filter every directory query
//! applies (`COALESCE(is_deactivated, FALSE) = FALSE`). Adding a `DELETE` against
//! a table nothing reads would be motion without effect, so it is deliberately
//! omitted — the flag is the mechanism, and it is set by step 6.
//!
//! # Error policy (deliberate, and asymmetric on purpose)
//!
//! * **Listing** the user's pending rooms fails ⇒ `Err`: without the list we
//!   cannot reject, the error is transient (a DB blip), and the user can retry.
//!   Neither the purge nor the teardown may have run.
//! * **Rejecting one invite/knock** fails ⇒ logged and skipped (upstream's
//!   `_reject_pending_invites_and_knocks_for_user` does the same): the failure
//!   is usually permanent for that room, and blocking deactivation on it would
//!   strand the user forever.
//! * **Enqueueing** the parting fails ⇒ `Err` (see step 3).
//! * **Listing** a queued user's joined rooms fails ⇒ the row is kept and the
//!   pass stops; the next tick retries instead of spinning.
//! * **Parting one joined room** fails ⇒ logged and skipped, and the row is
//!   still removed — matching upstream, which does not retry per room.

use std::sync::Arc;

use sqlx::PgPool;
use synapse_common::{ApiError, ApiResult};
use synapse_e2ee::backup::storage::KeyBackupStorage;
use synapse_e2ee::secure_backup::SecureBackupService;
use synapse_storage::account_data::AccountDataStorage;
use synapse_storage::push::{PushStorage, PushStoreApi};
use synapse_storage::room_account_data::RoomAccountDataStorage;
use synapse_storage::thread::ThreadStorage;
use synapse_storage::threepid::ThreepidStorage;
use synapse_storage::user_deactivation::UserDeactivationStorage;
use tokio::sync::Notify;

use crate::auth::CredentialAuth;
use crate::delayed_event_service::DelayedEventService;
use crate::room::RoomService;

/// The room-side operations account deactivation needs.
///
/// Narrower than [`RoomService`] on purpose: the pipeline needs exactly "which
/// rooms is this user in" and "take them out of one". Depending on the whole
/// `RoomService` would force every test of the *ordering* invariants above to
/// stand up the full membership machinery (state resolution, event writing,
/// summary refresh, four gates) and a database. Keeping this seam means the
/// order and the per-room error policy can be tested offline — which is where
/// they are actually easy to get wrong. Production passes the real
/// [`RoomService`]; see the delegation impl below.
#[async_trait::async_trait]
pub trait RoomParting: Send + Sync {
    /// Rooms `user_id` is currently joined to.
    async fn joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>>;

    /// Rooms `user_id` has a **pending** membership in — `invite` or `knock`,
    /// but not `join`. Disjoint from [`Self::joined_rooms`].
    ///
    /// Deactivation must reject these too; otherwise the account is gone but a
    /// pending invite keeps it addressable. Rejecting is just a self-`leave`
    /// ([`Self::part`]) — the membership state machine already allows that from
    /// `invite`/`knock`.
    async fn pending_rooms(&self, user_id: &str) -> ApiResult<Vec<String>>;

    /// Make `user_id` leave `room_id`.
    async fn part(&self, room_id: &str, user_id: &str) -> ApiResult<()>;
}

#[async_trait::async_trait]
impl RoomParting for RoomService {
    async fn joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.membership().get_joined_rooms(user_id).await
    }

    async fn pending_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.membership().get_pending_rooms(user_id).await
    }

    async fn part(&self, room_id: &str, user_id: &str) -> ApiResult<()> {
        // Through the normal membership path (transition state machine, event
        // admission gate, summary refresh, member-count decrement) rather than a
        // raw storage delete — a deactivation must not be the one code path that
        // skips the rules every other leave obeys.
        self.membership().leave_room(room_id, user_id).await
    }
}

/// The pending-deactivation queue (upstream's `user_pending_deactivations`).
///
/// A seam for the same reason [`RoomParting`] is one: the drain loop's
/// stop/retry semantics (keep the row when listing fails, drop it after a
/// per-room failure) are exactly what a fake can exercise offline.
#[async_trait::async_trait]
pub trait PendingDeactivations: Send + Sync {
    /// Enqueue `user_id`; idempotent.
    async fn enqueue(&self, user_id: &str) -> ApiResult<()>;

    /// The oldest queued user, if any.
    async fn next(&self) -> ApiResult<Option<String>>;

    /// Drop `user_id` from the queue once its parting pass has run.
    async fn remove(&self, user_id: &str) -> ApiResult<()>;
}

#[async_trait::async_trait]
impl PendingDeactivations for UserDeactivationStorage {
    async fn enqueue(&self, user_id: &str) -> ApiResult<()> {
        self.add_user_pending_deactivation(user_id).await
    }

    async fn next(&self) -> ApiResult<Option<String>> {
        self.get_user_pending_deactivation().await
    }

    async fn remove(&self, user_id: &str) -> ApiResult<()> {
        self.del_user_pending_deactivation(user_id).await.map(|_| ())
    }
}

/// What a deactivation actually did, for logging and for tests.
///
/// The joined-room parting is **not** counted here: it happens after this
/// report is returned (see [`AccountDeactivationService::process_pending_parts`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeactivationReport {
    /// MSC4140 delayed events cancelled.
    pub delayed_events_cancelled: usize,
    /// Pending invites/knocks rejected synchronously.
    pub pending_memberships_rejected: usize,
    /// Pending invites/knocks that could not be rejected (logged, not fatal).
    pub pending_memberships_failed: usize,
    /// Whether the user was enqueued for background parting of joined rooms.
    pub queued_for_parting: bool,
    /// Pusher rows deleted.
    pub pushers_deleted: u64,
    /// Account-scoped rows removed by [`UserDataPurger`], per table.
    pub purged: PurgedUserData,
}

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
}

/// The account-scoped cleanup half of the pipeline.
///
/// A seam for the same reason [`RoomParting`] is one: it lets the *pipeline*
/// (order, error policy, "irreversible step last") be tested offline with a fake,
/// while production uses [`StorageUserDataPurger`] over the real stores.
#[async_trait::async_trait]
pub trait UserDataPurger: Send + Sync {
    /// Remove every account-scoped row owned by `user_id` that deactivation is
    /// responsible for. See [`PurgedUserData`] for the tables.
    async fn purge(&self, user_id: &str) -> ApiResult<PurgedUserData>;
}

/// Production [`UserDataPurger`]: seven storage handles, all built from one pool.
///
/// Composing them here (rather than taking seven `Arc<dyn …>` constructor
/// arguments) keeps the container wiring to a single `new(&pool)` and keeps the
/// service's dependency list — the tests' fake surface — at one seam.
pub struct StorageUserDataPurger {
    account_data: AccountDataStorage,
    room_account_data: RoomAccountDataStorage,
    push_storage: PushStorage,
    thread_storage: ThreadStorage,
    threepid_storage: ThreepidStorage,
    key_backup_storage: KeyBackupStorage,
    secure_backup: SecureBackupService,
}

impl StorageUserDataPurger {
    /// Build every store from `pool`. Each storage's constructor takes the pool
    /// by one of three flavors (`&Arc`, `Arc`, `&`), so they are all fed from the
    /// same handle.
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self {
            account_data: AccountDataStorage::new(pool),
            room_account_data: RoomAccountDataStorage::new(pool),
            push_storage: PushStorage::new(pool.clone()),
            thread_storage: ThreadStorage::new(pool),
            threepid_storage: ThreepidStorage::new(pool.as_ref()),
            key_backup_storage: KeyBackupStorage::new(pool),
            secure_backup: SecureBackupService::new(pool),
        }
    }
}

#[async_trait::async_trait]
impl UserDataPurger for StorageUserDataPurger {
    async fn purge(&self, user_id: &str) -> ApiResult<PurgedUserData> {
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
        })
    }
}

/// Orchestrates the account-deactivation pipeline. See the module docs for the
/// step order, the two seams and the error policy.
pub struct AccountDeactivationService {
    /// Final, irreversible identity teardown (flag + tokens + devices). The
    /// authority is [`CredentialAuth::deactivate_user`]; this service adds the
    /// steps that have to happen *around* it.
    credential_auth: Arc<dyn CredentialAuth>,
    rooms: Arc<dyn RoomParting>,
    delayed_events: Arc<DelayedEventService>,
    push_storage: Arc<dyn PushStoreApi>,
    user_data: Arc<dyn UserDataPurger>,
    pending: Arc<dyn PendingDeactivations>,
    /// Wakes the background drain loop as soon as a deactivation enqueues work,
    /// instead of waiting for the next maintenance tick.
    parting_notify: Arc<Notify>,
}

impl AccountDeactivationService {
    /// See [`new`].
    pub fn new(
        credential_auth: Arc<dyn CredentialAuth>,
        rooms: Arc<dyn RoomParting>,
        delayed_events: Arc<DelayedEventService>,
        push_storage: Arc<dyn PushStoreApi>,
        user_data: Arc<dyn UserDataPurger>,
        pending: Arc<dyn PendingDeactivations>,
    ) -> Self {
        Self {
            credential_auth,
            rooms,
            delayed_events,
            push_storage,
            user_data,
            pending,
            parting_notify: Arc::new(Notify::new()),
        }
    }

    /// A handle the background loop can await to be woken after an enqueue.
    pub fn pending_parting_notify(&self) -> Arc<Notify> {
        self.parting_notify.clone()
    }

    /// Run the full deactivation pipeline for `user_id`.
    ///
    /// See the module docs for the order, the error policy, and why the
    /// irreversible step is last.
    #[::tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn deactivate(&self, user_id: &str) -> ApiResult<DeactivationReport> {
        // 1. MSC4140 first — see the module docs (upstream: "Do this first, so
        //    that none of them get sent while the rest of the deactivation is in
        //    progress"). Fail-fast: a scheduled message surviving deactivation is
        //    a security hole, not a data-hygiene issue.
        let delayed_events_cancelled = self.delayed_events.cancel_all_for_user(user_id).await?;

        // 2. Reject outstanding invites/knocks. Listing failure is fatal; a
        //    single room failing is not (see the module docs). This stays on the
        //    request path: the account must not remain addressable through a
        //    pending membership once deactivation has answered.
        let (pending_memberships_rejected, pending_memberships_failed) =
            self.reject_pending_memberships(user_id).await?;

        // 3. Enqueue the joined-room parting (upstream:
        //    `add_user_pending_deactivation`). Fail-fast: without this row
        //    nothing will ever part the user, and step 6 is irreversible.
        self.pending.enqueue(user_id).await?;

        // 4. Pushers. `pushers` has no FK to `devices`, so this is the only thing
        //    that removes them — upstream has the same explicit call.
        let pushers_deleted =
            self.push_storage.delete_all_pushers_for_user(user_id).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to delete pushers during account deactivation", e)
            })?;

        // 5. Account-scoped cleanup: account data, room account data, push rules,
        //    thread subscriptions, threepids, E2EE backups. Fail-fast — a
        //    half-purged account is a privacy leak, and the teardown below has not
        //    run yet, so a retry re-runs this from the top.
        let purged = self.user_data.purge(user_id).await?;

        // 6. The irreversible step, last so that a retry after a partial failure
        //    is meaningful.
        self.credential_auth.deactivate_user(user_id).await?;

        // Wake the background loop without awaiting it: the request must not
        // block on a user who is joined to thousands of rooms.
        self.parting_notify.notify_one();

        let report = DeactivationReport {
            delayed_events_cancelled,
            pending_memberships_rejected,
            pending_memberships_failed,
            queued_for_parting: true,
            pushers_deleted,
            purged,
        };
        ::tracing::info!(
            target: "security_audit",
            event = "account_deactivation_pipeline_completed",
            user_id = %user_id,
            delayed_events_cancelled,
            pending_memberships_rejected,
            pending_memberships_failed,
            pushers_deleted,
            account_data_deleted = purged.account_data_deleted,
            room_account_data_deleted = purged.room_account_data_deleted,
            push_rules_deleted = purged.push_rules_deleted,
            thread_subscriptions_deleted = purged.thread_subscriptions_deleted,
            threepids_removed = purged.threepids_removed,
            key_backups_deleted = purged.key_backups_deleted,
            secure_backups_deleted = purged.secure_backups_deleted,
            "Account deactivation pipeline completed; joined-room parting queued"
        );
        Ok(report)
    }

    /// Drain the pending-deactivation queue: part each queued account from the
    /// rooms it is still joined to, then delete its row.
    ///
    /// This is upstream's `_user_parter_loop`. Called at startup (restart
    /// recovery) and on every maintenance tick by the server, and woken
    /// immediately after a deactivation enqueues work.
    ///
    /// Returns the number of rooms the pass successfully left. Per-room failures
    /// are logged and skipped and the row is still removed (upstream does not
    /// retry per room); a failure to *list* a queued user's rooms keeps the row
    /// and surfaces as `Err`, stopping the pass so the next tick retries instead
    /// of spinning.
    #[::tracing::instrument(skip_all)]
    pub async fn process_pending_parts(&self) -> ApiResult<usize> {
        let mut parted = 0usize;

        while let Some(user_id) = self.pending.next().await? {
            match self.part_joined_rooms(&user_id).await {
                Ok((left, failed)) => {
                    parted += left;
                    self.pending.remove(&user_id).await?;
                    ::tracing::info!(
                        user_id = %user_id,
                        rooms_left = left,
                        rooms_failed = failed,
                        "Deactivated account parted from its joined rooms"
                    );
                }
                Err(error) => {
                    // Keep the row and stop this pass: the listing may be
                    // transiently unavailable, and retrying in a tight loop
                    // would only hammer the database.
                    ::tracing::warn!(
                        user_id = %user_id,
                        error = %error,
                        "Failed to list joined rooms for a pending deactivation; will retry"
                    );
                    return Err(error);
                }
            }
        }

        Ok(parted)
    }

    /// Reject every pending invite/knock, tolerating per-room failures.
    async fn reject_pending_memberships(&self, user_id: &str) -> ApiResult<(usize, usize)> {
        let targets = self.rooms.pending_rooms(user_id).await?;
        Ok(self.part_each(targets, user_id).await)
    }

    /// Leave every room the user is joined to, tolerating per-room failures.
    async fn part_joined_rooms(&self, user_id: &str) -> ApiResult<(usize, usize)> {
        let targets = self.rooms.joined_rooms(user_id).await?;
        Ok(self.part_each(targets, user_id).await)
    }

    /// Drive `leave` on each room, counting successes and failures.
    async fn part_each(&self, targets: Vec<String>, user_id: &str) -> (usize, usize) {
        let mut left = 0usize;
        let mut failed = 0usize;

        for room_id in targets {
            match self.rooms.part(&room_id, user_id).await {
                Ok(()) => left += 1,
                Err(error) => {
                    failed += 1;
                    // Not fatal on purpose: see the module docs. The room is named
                    // so an operator can part the user by hand if it matters.
                    ::tracing::warn!(
                        user_id = %user_id,
                        room_id = %room_id,
                        error = %error,
                        "Failed to part deactivated account from room; continuing"
                    );
                }
            }
        }

        (left, failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use synapse_storage::delayed_events::{CreateDelayedEventRequest, DelayedEventStorageApi};
    use synapse_storage::test_mocks::delayed_event::InMemoryDelayedEventStore;
    use synapse_storage::test_mocks::push::InMemoryPushStore;
    use synapse_storage::User;

    /// Ordered log shared by the fakes, so the tests can assert the *order* of the
    /// pipeline rather than just the final state.
    #[derive(Default)]
    struct Log(Mutex<Vec<String>>);

    impl Log {
        fn record(&self, entry: impl Into<String>) {
            self.0.lock().expect("log mutex").push(entry.into());
        }

        fn entries(&self) -> Vec<String> {
            self.0.lock().expect("log mutex").clone()
        }
    }

    /// `RoomParting` fake: records the calls, and can be told to fail the listing
    /// or specific rooms.
    ///
    /// `fail_listing` is an `AtomicBool` so a test can flip it *after* the
    /// deactivation request has passed its own listing (the pending-invite step)
    /// and observe the drain loop's retry semantics.
    struct FakeRooms {
        joined: Vec<String>,
        pending: Vec<String>,
        fail_listing: AtomicBool,
        fail_rooms: Vec<String>,
        log: Arc<Log>,
    }

    #[async_trait::async_trait]
    impl RoomParting for FakeRooms {
        async fn joined_rooms(&self, _user_id: &str) -> ApiResult<Vec<String>> {
            if self.fail_listing.load(Ordering::SeqCst) {
                return Err(ApiError::internal("listing rooms failed"));
            }
            Ok(self.joined.clone())
        }

        async fn pending_rooms(&self, _user_id: &str) -> ApiResult<Vec<String>> {
            if self.fail_listing.load(Ordering::SeqCst) {
                return Err(ApiError::internal("listing rooms failed"));
            }
            Ok(self.pending.clone())
        }

        async fn part(&self, room_id: &str, _user_id: &str) -> ApiResult<()> {
            self.log.record(format!("part:{room_id}"));
            if self.fail_rooms.iter().any(|room| room == room_id) {
                return Err(ApiError::internal("parting failed"));
            }
            Ok(())
        }
    }

    /// `PendingDeactivations` fake: an in-memory FIFO queue plus a call log.
    struct FakePending {
        queue: Mutex<VecDeque<String>>,
        log: Arc<Log>,
    }

    impl FakePending {
        /// Queue a user without the `enqueue:` log entry, so a drain-loop test
        /// can start from a non-empty queue without having to run `deactivate`.
        fn seed(&self, user_id: &str) {
            self.queue.lock().expect("pending mutex").push_back(user_id.to_string());
        }

        fn queued(&self) -> Vec<String> {
            self.queue.lock().expect("pending mutex").iter().cloned().collect()
        }
    }

    #[async_trait::async_trait]
    impl PendingDeactivations for FakePending {
        async fn enqueue(&self, user_id: &str) -> ApiResult<()> {
            self.log.record(format!("enqueue:{user_id}"));
            let mut queue = self.queue.lock().expect("pending mutex");
            if !queue.iter().any(|queued| queued == user_id) {
                queue.push_back(user_id.to_string());
            }
            Ok(())
        }

        async fn next(&self) -> ApiResult<Option<String>> {
            Ok(self.queue.lock().expect("pending mutex").front().cloned())
        }

        async fn remove(&self, user_id: &str) -> ApiResult<()> {
            self.log.record(format!("dequeue:{user_id}"));
            self.queue.lock().expect("pending mutex").retain(|queued| queued != user_id);
            Ok(())
        }
    }

    /// `UserDataPurger` fake: records the call and returns a caller-set count.
    struct FakePurger {
        log: Arc<Log>,
        purged: PurgedUserData,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl UserDataPurger for FakePurger {
        async fn purge(&self, _user_id: &str) -> ApiResult<PurgedUserData> {
            self.log.record("purge");
            if self.fail {
                return Err(ApiError::internal("purge failed"));
            }
            Ok(self.purged)
        }
    }

    /// `CredentialAuth` fake: only `deactivate_user` is exercised here; the rest
    /// return a "not configured" error like the crate's other mocks, so a test can
    /// never accidentally rely on behaviour it did not set up.
    struct FakeCredentialAuth {
        log: Arc<Log>,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl CredentialAuth for FakeCredentialAuth {
        async fn deactivate_user(&self, _user_id: &str) -> ApiResult<()> {
            self.log.record("deactivate");
            if self.fail {
                return Err(ApiError::internal("deactivate_user failed"));
            }
            Ok(())
        }

        async fn login(
            &self,
            _username: &str,
            _password: &str,
            _device_id: Option<&str>,
            _initial_display_name: Option<&str>,
        ) -> ApiResult<(User, String, String, String)> {
            Err(ApiError::internal("not configured"))
        }

        async fn register(
            &self,
            _username: &str,
            _password: &str,
            _admin: bool,
            _displayname: Option<&str>,
        ) -> ApiResult<(User, String, String, String)> {
            Err(ApiError::internal("not configured"))
        }

        async fn register_with_device_name(
            &self,
            _username: &str,
            _password: &str,
            _admin: bool,
            _displayname: Option<&str>,
            _device_display_name: Option<&str>,
        ) -> ApiResult<(User, String, String, String)> {
            Err(ApiError::internal("not configured"))
        }

        async fn change_password(
            &self,
            _user_id: &str,
            _current_password: Option<&str>,
            _new_password: &str,
            _current_device_id: Option<&str>,
            _logout_devices: bool,
        ) -> ApiResult<()> {
            Err(ApiError::internal("not configured"))
        }

        async fn verify_user_credentials(&self, _user_id: &str, _password: &str) -> ApiResult<()> {
            Err(ApiError::internal("not configured"))
        }

        async fn register_guest_account(
            &self,
            _device_id: Option<&str>,
            _display_name: Option<&str>,
        ) -> ApiResult<(User, String, String)> {
            Err(ApiError::internal("not configured"))
        }

        async fn require_guest_user(&self, _user_id: &str) -> ApiResult<User> {
            Err(ApiError::internal("not configured"))
        }

        async fn upgrade_guest_account(
            &self,
            _user_id: &str,
            _device_id: Option<&str>,
            _username: &str,
            _password: &str,
        ) -> ApiResult<String> {
            Err(ApiError::internal("not configured"))
        }

        fn generate_email_verification_token(&self) -> ApiResult<String> {
            Err(ApiError::internal("not configured"))
        }
    }

    struct Harness {
        service: AccountDeactivationService,
        delayed_store: Arc<InMemoryDelayedEventStore>,
        push_store: Arc<InMemoryPushStore>,
        rooms: Arc<FakeRooms>,
        pending: Arc<FakePending>,
        log: Arc<Log>,
    }

    /// Fakes the pipeline's two data-cleaning seams. The common case (nothing to
    /// purge, no failure) is [`Default`], so most tests can pass `PurgerSpec::default()`.
    #[derive(Default)]
    struct PurgerSpec {
        purged: PurgedUserData,
        fail: bool,
    }

    fn harness(
        joined: Vec<&str>,
        pending: Vec<&str>,
        fail_listing: bool,
        fail_rooms: Vec<&str>,
        credential_auth_fails: bool,
        purger: PurgerSpec,
    ) -> Harness {
        let log = Arc::new(Log::default());
        let delayed_store = Arc::new(InMemoryDelayedEventStore::new());
        let push_store = Arc::new(InMemoryPushStore::new());
        let rooms = Arc::new(FakeRooms {
            joined: joined.into_iter().map(str::to_string).collect(),
            pending: pending.into_iter().map(str::to_string).collect(),
            fail_listing: AtomicBool::new(fail_listing),
            fail_rooms: fail_rooms.into_iter().map(str::to_string).collect(),
            log: log.clone(),
        });
        let pending_queue = Arc::new(FakePending { queue: Mutex::new(VecDeque::new()), log: log.clone() });

        let service = AccountDeactivationService::new(
            Arc::new(FakeCredentialAuth { log: log.clone(), fail: credential_auth_fails }),
            rooms.clone(),
            Arc::new(DelayedEventService::new(delayed_store.clone())),
            push_store.clone(),
            Arc::new(FakePurger { log: log.clone(), purged: purger.purged, fail: purger.fail }),
            pending_queue.clone(),
        );

        Harness { service, delayed_store, push_store, rooms, pending: pending_queue, log }
    }

    async fn seed_delayed_event(store: &InMemoryDelayedEventStore, user_id: &str) -> i64 {
        store
            .create_delayed_event(CreateDelayedEventRequest {
                room_id: "!room:example.com".to_string(),
                user_id: user_id.to_string(),
                device_id: "DEVICE".to_string(),
                event_type: "m.room.message".to_string(),
                state_key: None,
                content: serde_json::json!({ "body": "later" }),
                delay_ms: 60_000,
                sticky_duration_ms: None,
            })
            .await
            .expect("seed delayed event")
            .id
    }

    #[tokio::test]
    async fn deactivate_cancels_delayed_events_enqueues_parting_and_tears_down() {
        let h = harness(vec!["!a:example.com", "!b:example.com"], vec![], false, vec![], false, PurgerSpec::default());
        let delayed = seed_delayed_event(&h.delayed_store, "@alice:example.com").await;

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report.delayed_events_cancelled, 1);
        assert!(report.queued_for_parting, "the joined-room parting must be queued, not run inline");
        assert!(
            !h.log.entries().iter().any(|entry| entry.starts_with("part:")),
            "the request must not block on per-room parting: {:?}",
            h.log.entries()
        );
        assert_eq!(
            h.delayed_store.status_of(delayed).await.as_deref(),
            Some("cancelled"),
            "the scheduled message must not survive deactivation"
        );

        let parted = h.service.process_pending_parts().await.expect("drain the queue");

        assert_eq!(parted, 2);
        assert_eq!(
            h.log.entries(),
            vec![
                "enqueue:@alice:example.com",
                "purge",
                "deactivate",
                "part:!a:example.com",
                "part:!b:example.com",
                "dequeue:@alice:example.com",
            ]
        );
        assert!(h.pending.queued().is_empty(), "the row must be removed once the pass ran");
    }

    /// The invariant the inversion in the module docs buys: the irreversible step
    /// runs **last**, so a retry after a partial failure is meaningful. A test
    /// that only checked the final state would pass even if `deactivate` ran first.
    ///
    /// The joined-room parting is not part of this ordering at all — it is the
    /// *background* drain's job (see the module docs), so it must not appear on
    /// the request path.
    #[tokio::test]
    async fn irreversible_teardown_is_the_last_synchronous_step() {
        let h = harness(vec!["!a:example.com"], vec![], false, vec![], false, PurgerSpec::default());

        h.service.deactivate("@alice:example.com").await.expect("deactivate");

        let entries = h.log.entries();
        let teardown_at = entries.iter().position(|e| e == "deactivate").expect("teardown must be logged");
        assert_eq!(teardown_at, entries.len() - 1, "teardown must come after every synchronous step: {entries:?}");
        assert!(
            entries[..teardown_at].iter().all(|e| e.starts_with("enqueue:") || e == "purge"),
            "only the reversible cleanup steps may precede the teardown: {entries:?}"
        );
    }

    /// Upstream does not retry per room: a room that cannot be left is logged and
    /// skipped, and the queue row is still removed. Otherwise one permanently
    /// unpartable room would wedge the whole queue forever.
    #[tokio::test]
    async fn a_single_unpartable_room_does_not_block_the_parting_pass() {
        let h = harness(
            vec!["!good:example.com", "!broken:example.com"],
            vec![],
            false,
            vec!["!broken:example.com"],
            false,
            PurgerSpec::default(),
        );

        h.service.deactivate("@alice:example.com").await.expect("deactivation must still complete");
        let parted = h.service.process_pending_parts().await.expect("the pass must run");

        assert_eq!(parted, 1, "only the healthy room counts as parted");
        let entries = h.log.entries();
        for room in ["part:!good:example.com", "part:!broken:example.com"] {
            assert!(entries.contains(&room.to_string()), "missing {room}: {entries:?}");
        }
        assert!(h.pending.queued().is_empty(), "the row must be removed even when one room failed");
    }

    /// Fail-closed: without the room list we cannot reject outstanding invites, and
    /// the error is transient — the user retries. Nothing reversible, and above all
    /// nothing irreversible, may have run.
    #[tokio::test]
    async fn a_failed_room_listing_aborts_before_the_irreversible_step() {
        let h = harness(vec![], vec![], true, vec![], false, PurgerSpec::default());

        let err = h.service.deactivate("@alice:example.com").await.expect_err("listing failure must be fatal");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
        assert!(
            !h.log.entries().iter().any(|e| e == "deactivate"),
            "must not deactivate when parting could not even be attempted"
        );
        assert!(
            !h.log.entries().iter().any(|e| e == "purge"),
            "must not purge when parting could not even be attempted"
        );
        assert!(
            !h.log.entries().iter().any(|e| e.starts_with("enqueue:")),
            "must not enqueue parting when the pending-membership pass already failed"
        );
    }

    #[tokio::test]
    async fn a_failed_teardown_surfaces_as_an_error() {
        let h = harness(vec!["!a:example.com"], vec![], false, vec![], true, PurgerSpec::default());

        let err = h.service.deactivate("@alice:example.com").await.expect_err("teardown failure must propagate");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
    }

    #[tokio::test]
    async fn deactivate_without_rooms_or_delayed_events_still_enqueues_and_tears_down() {
        let h = harness(vec![], vec![], false, vec![], false, PurgerSpec::default());

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report, DeactivationReport { queued_for_parting: true, ..DeactivationReport::default() });
        assert_eq!(h.log.entries(), vec!["enqueue:@alice:example.com", "purge", "deactivate"]);

        // The row is still enqueued even with nothing to part (the queue is
        // written before the joined-room list is ever consulted), and the drain
        // removes it after a no-op pass.
        assert_eq!(h.service.process_pending_parts().await.expect("drain"), 0);
        assert!(h.pending.queued().is_empty());
    }

    /// The pusher step must reach the storage seam: `deactivate` has to delete the
    /// user's pushers, and `pushers` has no FK to `devices` to fall back on.
    #[tokio::test]
    async fn deactivate_deletes_the_users_pushers() {
        let h = harness(vec![], vec![], false, vec![], false, PurgerSpec::default());
        h.push_store
            .upsert_pusher(
                "@alice:example.com",
                "DEVICE",
                "pushkey-1",
                "http",
                "app.id",
                "App",
                "Dev",
                &None,
                "en",
                &None,
                0,
            )
            .await
            .expect("seed pusher");

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report.pushers_deleted, 1, "the seeded pusher must be deleted");
        assert!(
            h.push_store.get_pushers("@alice:example.com", Some("DEVICE")).await.expect("read pushers").is_empty(),
            "no pusher may survive deactivation"
        );
    }

    /// A deactivation must reject outstanding invites and retract knocks, not just
    /// enqueue joined-room parting — otherwise the account is gone but still
    /// addressable through a pending membership. These stay on the request path
    /// (upstream does the same); only `join` memberships go to the background.
    #[tokio::test]
    async fn deactivate_rejects_pending_invites_and_knocks() {
        let h = harness(
            vec!["!joined:example.com"],
            vec!["!invite:example.com", "!knock:example.com"],
            false,
            vec![],
            false,
            PurgerSpec::default(),
        );

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report.pending_memberships_rejected, 2, "both pending memberships must be rejected");
        assert_eq!(report.pending_memberships_failed, 0);
        let entries = h.log.entries();
        for room in ["part:!invite:example.com", "part:!knock:example.com"] {
            assert!(entries.contains(&room.to_string()), "missing {room}: {entries:?}");
        }
        assert!(
            !entries.contains(&"part:!joined:example.com".to_string()),
            "joined rooms must be left by the background drain, not inline: {entries:?}"
        );

        h.service.process_pending_parts().await.expect("drain the queue");

        assert!(
            h.log.entries().contains(&"part:!joined:example.com".to_string()),
            "the drain must part the joined room: {:?}",
            h.log.entries()
        );
    }

    /// The purge step must reach the storage seam, and its per-table counts must
    /// surface on the report so an operator can see what was actually removed.
    #[tokio::test]
    async fn deactivate_purges_account_data_and_reports_the_counts() {
        let purged = PurgedUserData {
            account_data_deleted: 2,
            room_account_data_deleted: 3,
            push_rules_deleted: 4,
            thread_subscriptions_deleted: 5,
            threepids_removed: 1,
            key_backups_deleted: 1,
            secure_backups_deleted: 1,
        };
        let h = harness(vec![], vec![], false, vec![], false, PurgerSpec { purged, fail: false });

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report.purged, purged, "the purge counts must be carried on the report");
        assert!(h.log.entries().contains(&"purge".to_string()), "the purge seam must be exercised");
    }

    /// Fail-fast: a purge failure is a privacy leak in the making, so it must abort
    /// before the irreversible teardown — leaving a retryable, not-yet-deactivated
    /// account rather than a deactivated one that still holds data.
    #[tokio::test]
    async fn a_failed_purge_aborts_before_the_irreversible_step() {
        let h =
            harness(vec![], vec![], false, vec![], false, PurgerSpec { purged: PurgedUserData::default(), fail: true });

        let err = h.service.deactivate("@alice:example.com").await.expect_err("purge failure must be fatal");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
        assert!(
            !h.log.entries().iter().any(|e| e == "deactivate"),
            "the teardown must not run when the purge failed: {:?}",
            h.log.entries()
        );
    }

    /// A queued user whose room list cannot be read must **stay** in the queue:
    /// dropping the row would lose the parting work forever. The pass stops
    /// instead of spinning, and the next tick retries.
    #[tokio::test]
    async fn process_pending_parts_keeps_the_row_when_listing_fails() {
        let h = harness(vec!["!a:example.com"], vec![], false, vec![], false, PurgerSpec::default());
        h.pending.seed("@alice:example.com");
        h.rooms.fail_listing.store(true, Ordering::SeqCst);

        let err = h.service.process_pending_parts().await.expect_err("a listing failure must surface");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
        assert_eq!(
            h.pending.queued(),
            vec!["@alice:example.com".to_string()],
            "the row must survive so the next tick can retry"
        );
    }

    /// The loop drains the whole queue, not just the head: a pass is not done
    /// until every queued account has had its parting attempt and its row
    /// removed.
    #[tokio::test]
    async fn process_pending_parts_drains_every_queued_user() {
        let h = harness(vec![], vec![], false, vec![], false, PurgerSpec::default());
        h.pending.seed("@alice:example.com");
        h.pending.seed("@bob:example.com");

        let parted = h.service.process_pending_parts().await.expect("drain");

        assert_eq!(parted, 0, "neither account is joined to anything");
        assert!(h.pending.queued().is_empty(), "both rows must be removed");
        assert_eq!(
            h.log.entries(),
            vec!["dequeue:@alice:example.com", "dequeue:@bob:example.com"],
            "the drain must visit every queued user in FIFO order"
        );
    }

    /// The request must not wait for the next maintenance tick (60s by default):
    /// the enqueue wakes the drain loop directly.
    #[tokio::test]
    async fn deactivate_wakes_the_background_drain_loop() {
        let h = harness(vec![], vec![], false, vec![], false, PurgerSpec::default());
        let notify = h.service.pending_parting_notify();

        h.service.deactivate("@alice:example.com").await.expect("deactivate");

        tokio::time::timeout(std::time::Duration::from_millis(100), notify.notified())
            .await
            .expect("deactivation must wake the drain loop instead of waiting for the next tick");
    }
}
