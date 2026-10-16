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
//!   the devices does not remove them.
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
//! 2. part every joined room (**best-effort per room**, fail-fast on listing);
//! 3. delete all pushers (**fail-fast**);
//! 4. revoke tokens/devices and mark deactivated (**fail-fast, last**).
//!
//! Putting the irreversible step last means a retry after a partial failure is
//! meaningful: the user can press "deactivate" again and the pipeline re-runs
//! from the top. Doing it first would leave an account that is deactivated but
//! still in rooms, with no mechanism to finish the job.
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

use synapse_common::{ApiError, ApiResult};
use synapse_storage::push::PushStoreApi;

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

    /// Make `user_id` leave `room_id`.
    async fn part(&self, room_id: &str, user_id: &str) -> ApiResult<()>;
}

#[async_trait::async_trait]
impl RoomParting for RoomService {
    async fn joined_rooms(&self, user_id: &str) -> ApiResult<Vec<String>> {
        self.membership().get_joined_rooms(user_id).await
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
}

impl AccountDeactivationService {
    /// See [`new`].
    pub fn new(
        credential_auth: Arc<dyn CredentialAuth>,
        rooms: Arc<dyn RoomParting>,
        delayed_events: Arc<DelayedEventService>,
        push_storage: Arc<dyn PushStoreApi>,
    ) -> Self {
        Self { credential_auth, rooms, delayed_events, push_storage }
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

        // 4. The irreversible step, last so that a retry after a partial failure
        //    is meaningful.
        self.credential_auth.deactivate_user(user_id).await?;

        let report = DeactivationReport { delayed_events_cancelled, rooms_left, rooms_failed, pushers_deleted };
        ::tracing::info!(
            target: "security_audit",
            event = "account_deactivation_pipeline_completed",
            user_id = %user_id,
            delayed_events_cancelled,
            rooms_left,
            rooms_failed,
            pushers_deleted,
            "Account deactivation pipeline completed"
        );
        Ok(report)
    }

    /// Leave every room the user is joined to, tolerating per-room failures.
    async fn part_rooms(&self, user_id: &str) -> ApiResult<(usize, usize)> {
        let joined = self.rooms.joined_rooms(user_id).await?;
        let mut left = 0usize;
        let mut failed = 0usize;

        for room_id in joined {
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

        async fn part(&self, room_id: &str, _user_id: &str) -> ApiResult<()> {
            self.log.record(format!("part:{room_id}"));
            if self.fail_rooms.iter().any(|room| room == room_id) {
                return Err(ApiError::internal("parting failed"));
            }
            Ok(())
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

    fn harness(joined: Vec<&str>, fail_listing: bool, fail_rooms: Vec<&str>, credential_auth_fails: bool) -> Harness {
        let log = Arc::new(Log::default());
        let delayed_store = Arc::new(InMemoryDelayedEventStore::new());
        let push_store = Arc::new(InMemoryPushStore::new());

        let service = AccountDeactivationService::new(
            Arc::new(FakeCredentialAuth { log: log.clone(), fail: credential_auth_fails }),
            Arc::new(FakeRooms {
                joined: joined.into_iter().map(str::to_string).collect(),
                fail_listing,
                fail_rooms: fail_rooms.into_iter().map(str::to_string).collect(),
                log: log.clone(),
            }),
            Arc::new(DelayedEventService::new(delayed_store.clone())),
            push_store.clone(),
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
            })
            .await
            .expect("seed delayed event")
            .id
    }

    #[tokio::test]
    async fn deactivate_cancels_delayed_events_parts_rooms_and_tears_down() {
        let h = harness(vec!["!a:example.com", "!b:example.com"], false, vec![], false);
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
        assert_eq!(h.log.entries(), vec!["part:!a:example.com", "part:!b:example.com", "deactivate"]);
    }

    /// The invariant the inversion in the module docs buys: the irreversible step
    /// runs **last**, so a retry after a partial failure is meaningful. A test
    /// that only checked the final state would pass even if `deactivate` ran first.
    #[tokio::test]
    async fn irreversible_teardown_is_the_last_step() {
        let h = harness(vec!["!a:example.com"], false, vec![], false);

        h.service.deactivate("@alice:example.com").await.expect("deactivate");

        let entries = h.log.entries();
        let teardown_at = entries.iter().position(|e| e == "deactivate").expect("teardown must be logged");
        assert_eq!(teardown_at, entries.len() - 1, "teardown must come after every parting attempt: {entries:?}");
        assert!(entries[..teardown_at].iter().all(|e| e.starts_with("part:")), "{entries:?}");
    }

    #[tokio::test]
    async fn a_single_unpartable_room_does_not_block_deactivation() {
        let h = harness(vec!["!good:example.com", "!broken:example.com"], false, vec!["!broken:example.com"], false);

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivation must still complete");

        assert_eq!(report.rooms_left, 1);
        assert_eq!(report.rooms_failed, 1);
        assert!(h.log.entries().contains(&"deactivate".to_string()), "the teardown must still run");
    }

    /// Fail-closed: without the room list we cannot part, and the error is
    /// transient — the user retries. The irreversible step must NOT have run.
    #[tokio::test]
    async fn a_failed_room_listing_aborts_before_the_irreversible_step() {
        let h = harness(vec![], true, vec![], false);

        let err = h.service.deactivate("@alice:example.com").await.expect_err("listing failure must be fatal");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
        assert!(
            !h.log.entries().iter().any(|e| e == "deactivate"),
            "must not deactivate when parting could not even be attempted"
        );
    }

    #[tokio::test]
    async fn a_failed_teardown_surfaces_as_an_error() {
        let h = harness(vec!["!a:example.com"], false, vec![], true);

        let err = h.service.deactivate("@alice:example.com").await.expect_err("teardown failure must propagate");

        assert_eq!(err.kind, synapse_common::ApiErrorKind::Internal);
    }

    #[tokio::test]
    async fn deactivate_is_a_noop_without_rooms_or_delayed_events() {
        let h = harness(vec![], false, vec![], false);

        let report = h.service.deactivate("@alice:example.com").await.expect("deactivate");

        assert_eq!(report, DeactivationReport::default());
        assert_eq!(h.log.entries(), vec!["deactivate"]);
    }

    /// The pusher step must reach the storage seam: `deactivate` has to delete the
    /// user's pushers, and `pushers` has no FK to `devices` to fall back on.
    #[tokio::test]
    async fn deactivate_deletes_the_users_pushers() {
        let h = harness(vec![], false, vec![], false);
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
}
