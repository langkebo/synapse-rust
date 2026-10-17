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
//! # Ordering, and where it deviates from upstream
//!
//! Upstream marks the account deactivated and then parts the user from their
//! rooms **asynchronously** (a `add_user_pending_deactivation` row plus a
//! background parting loop that retries), so a parting failure never blocks the
//! deactivation. This repo has no such retry table, so we invert the last two
//! steps:
//!
//! 1. cancel delayed events (**fail-fast**, security-critical);
//! 2. part every joined room **and reject every pending invite/knock**
//!    (**best-effort per room**, fail-fast on listing);
//! 3. delete all pushers (**fail-fast**);
//! 4. purge the account-scoped rows — account data, room account data, push
//!    rules, thread subscriptions, threepids and E2EE backups (**fail-fast**);
//! 5. revoke tokens/devices and mark deactivated (**fail-fast, last**).
//!
//! Putting the irreversible step last means a retry after a partial failure is
//! meaningful: the user can press "deactivate" again and the pipeline re-runs
//! from the top. Doing it first would leave an account that is deactivated but
//! still in rooms, with no mechanism to finish the job.
//!
//! # Rejecting pending invites/knocks
//!
//! Step 2 targets the union of *joined* and *pending* (`invite` / `knock`) rooms.
//! A pending membership is not a `join`, so the old pipeline skipped it and the
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
//! omitted — the flag is the mechanism, and it is set by step 5.
//!
//! # Error policy (deliberate, and asymmetric on purpose)
//!
//! * **Listing** the user's rooms fails ⇒ `Err`: without the list we cannot part
//!   at all, the error is transient (a DB blip), and the user can retry.
//! * **Parting one room** fails ⇒ logged and skipped: the failure is usually
//!   permanent for that room (a broken room, an admission-gate refusal, an
//!   unreachable remote). Blocking deactivation on it would strand the user
//!   forever, which is exactly what upstream's async loop avoids.

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

/// What a deactivation actually did, for logging and for tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeactivationReport {
    /// MSC4140 delayed events cancelled.
    pub delayed_events_cancelled: usize,
    /// Rooms the user was successfully parted from.
    pub rooms_left: usize,
    /// Rooms the parting attempt failed for (logged, not fatal).
    pub rooms_failed: usize,
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
}

impl AccountDeactivationService {
    /// See [`new`].
    pub fn new(
        credential_auth: Arc<dyn CredentialAuth>,
        rooms: Arc<dyn RoomParting>,
        delayed_events: Arc<DelayedEventService>,
        push_storage: Arc<dyn PushStoreApi>,
        user_data: Arc<dyn UserDataPurger>,
    ) -> Self {
        Self { credential_auth, rooms, delayed_events, push_storage, user_data }
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

        // 2. Part the user from their rooms. Listing failure is fatal; a single
        //    room failing is not (see the module docs).
        let (rooms_left, rooms_failed) = self.part_rooms(user_id).await?;

        // 3. Pushers. `pushers` has no FK to `devices`, so this is the only thing
        //    that removes them — upstream has the same explicit call.
        let pushers_deleted =
            self.push_storage.delete_all_pushers_for_user(user_id).await.map_err(|e| {
                ApiError::internal_with_cause("Failed to delete pushers during account deactivation", e)
            })?;

        // 4. Account-scoped cleanup: account data, room account data, push rules,
        //    thread subscriptions, threepids, E2EE backups. Fail-fast — a
        //    half-purged account is a privacy leak, and the teardown below has not
        //    run yet, so a retry re-runs this from the top.
        let purged = self.user_data.purge(user_id).await?;

        // 5. The irreversible step, last so that a retry after a partial failure
        //    is meaningful.
        self.credential_auth.deactivate_user(user_id).await?;

        let report = DeactivationReport { delayed_events_cancelled, rooms_left, rooms_failed, pushers_deleted, purged };
        ::tracing::info!(
            target: "security_audit",
            event = "account_deactivation_pipeline_completed",
            user_id = %user_id,
            delayed_events_cancelled,
            rooms_left,
            rooms_failed,
            pushers_deleted,
            account_data_deleted = purged.account_data_deleted,
            room_account_data_deleted = purged.room_account_data_deleted,
            push_rules_deleted = purged.push_rules_deleted,
            thread_subscriptions_deleted = purged.thread_subscriptions_deleted,
            threepids_removed = purged.threepids_removed,
            key_backups_deleted = purged.key_backups_deleted,
            secure_backups_deleted = purged.secure_backups_deleted,
            "Account deactivation pipeline completed"
        );
        Ok(report)
    }

    /// Leave every room the user is joined to, and reject every pending
    /// invite/knock, tolerating per-room failures.
    async fn part_rooms(&self, user_id: &str) -> ApiResult<(usize, usize)> {
        // `joined` and `pending` are disjoint by construction (a membership is one
        // of join/invite/knock), so the union has no duplicates to dedupe.
        let mut targets = self.rooms.joined_rooms(user_id).await?;
        targets.extend(self.rooms.pending_rooms(user_id).await?);

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

        Ok((left, failed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    struct FakeRooms {
        joined: Vec<String>,
        pending: Vec<String>,
        fail_listing: bool,
        fail_rooms: Vec<String>,
        log: Arc<Log>,
    }

    #[async_trait::async_trait]
    impl RoomParting for FakeRooms {
        async fn joined_rooms(&self, _user_id: &str) -> ApiResult<Vec<String>> {
            if self.fail_listing {
                return Err(ApiError::internal("listing rooms failed"));
            }
            Ok(self.joined.clone())
        }

        async fn pending_rooms(&self, _user_id: &str) -> ApiResult<Vec<String>> {
            if self.fail_listing {
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

        async fn register_guest_account(&self) -> ApiResult<(User, String, String)> {
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

        let service = AccountDeactivationService::new(
            Arc::new(FakeCredentialAuth { log: log.clone(), fail: credential_auth_fails }),
            Arc::new(FakeRooms {
                joined: joined.into_iter().map(str::to_string).collect(),
                pending: pending.into_iter().map(str::to_string).collect(),
                fail_listing,
                fail_rooms: fail_rooms.into_iter().map(str::to_string).collect(),
                log: log.clone(),
            }),
            Arc::new(DelayedEventService::new(delayed_store.clone())),
            push_store.clone(),
            Arc::new(FakePurger { log: log.clone(), purged: purger.purged, fail: purger.fail }),
        );

        Harness { service, delayed_store, push_store, log }
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
    async fn deactivate_cancels_delayed_events_parts_rooms_and_tears_down() {
        let h = harness(vec!["!a:example.com", "!b:example.com"], vec![], false, vec![], false, PurgerSpec::default());
        let delayed = seed_delayed_event(&h.delayed_store, "@alice:example.com").await;

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report.delayed_events_cancelled, 1);
        assert_eq!(report.rooms_left, 2);
        assert_eq!(report.rooms_failed, 0);
        assert_eq!(
            h.delayed_store.status_of(delayed).await.as_deref(),
            Some("cancelled"),
            "the scheduled message must not survive deactivation"
        );
        assert_eq!(h.log.entries(), vec!["part:!a:example.com", "part:!b:example.com", "purge", "deactivate"]);
    }

    /// The invariant the inversion in the module docs buys: the irreversible step
    /// runs **last**, so a retry after a partial failure is meaningful. A test
    /// that only checked the final state would pass even if `deactivate` ran first.
    #[tokio::test]
    async fn irreversible_teardown_is_the_last_step() {
        let h = harness(vec!["!a:example.com"], vec![], false, vec![], false, PurgerSpec::default());

        h.service.deactivate("@alice:example.com").await.expect("deactivate");

        let entries = h.log.entries();
        let teardown_at = entries.iter().position(|e| e == "deactivate").expect("teardown must be logged");
        assert_eq!(teardown_at, entries.len() - 1, "teardown must come after every parting attempt: {entries:?}");
        assert!(
            entries[..teardown_at].iter().all(|e| e.starts_with("part:") || e == "purge"),
            "only the reversible cleanup steps may precede the teardown: {entries:?}"
        );
    }

    #[tokio::test]
    async fn a_single_unpartable_room_does_not_block_deactivation() {
        let h = harness(
            vec!["!good:example.com", "!broken:example.com"],
            vec![],
            false,
            vec!["!broken:example.com"],
            false,
            PurgerSpec::default(),
        );

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivation must still complete");

        assert_eq!(report.rooms_left, 1);
        assert_eq!(report.rooms_failed, 1);
        assert!(h.log.entries().contains(&"deactivate".to_string()), "the teardown must still run");
    }

    /// Fail-closed: without the room list we cannot part, and the error is
    /// transient — the user retries. Neither the purge nor the irreversible step
    /// must have run.
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
    }

    #[tokio::test]
    async fn a_failed_teardown_surfaces_as_an_error() {
        let h = harness(vec!["!a:example.com"], vec![], false, vec![], true, PurgerSpec::default());

        let err = h.service.deactivate("@alice:example.com").await.expect_err("teardown failure must propagate");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
    }

    #[tokio::test]
    async fn deactivate_is_a_noop_without_rooms_or_delayed_events() {
        let h = harness(vec![], vec![], false, vec![], false, PurgerSpec::default());

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report, DeactivationReport::default());
        assert_eq!(h.log.entries(), vec!["purge", "deactivate"]);
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
    /// part joined rooms — otherwise the account is gone but still addressable
    /// through a pending membership.
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

        assert_eq!(report.rooms_left, 3, "the joined room and both pending rooms must be left");
        assert_eq!(report.rooms_failed, 0);
        let entries = h.log.entries();
        for room in ["part:!joined:example.com", "part:!invite:example.com", "part:!knock:example.com"] {
            assert!(entries.contains(&room.to_string()), "missing {room}: {entries:?}");
        }
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
}
