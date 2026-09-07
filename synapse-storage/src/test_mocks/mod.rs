//! Pre-positioned Mock adapters for the storage layer.
//!
//! Aggregates in-memory test doubles for storage traits so service-layer
//! unit tests can run without a real PostgreSQL pool. Engineers extend
//! these mocks by adding methods that mirror the production trait surface.
//!
//! See `.claude/skills/tdd-rust/SKILL.md` §4 for usage rules and
//! `.trae/documents/TDD落地执行清单.md` Phase 3 for the extension plan.
//!
//! # Existing patterns
//!
//! - [`crate::UserStore`] trait + [`crate::UserStorage`] (Postgres) + [`FakeUserStore`] (in-memory)
//! - Engineers should follow the same trait + 2-impl seam for new storage mocks.

pub use crate::user_store_fake::FakeUserStore;

use std::sync::Arc;

/// Type alias for ergonomic service-layer injection.
pub type SharedFakeUserStore = Arc<FakeUserStore>;

/// Construct an [`Arc<FakeUserStore>`] ready for injection into services
/// that expect `Arc<dyn UserStore>`.
pub fn shared_fake_user_store() -> SharedFakeUserStore {
    Arc::new(FakeUserStore::new())
}

/// Async seeding helper — applies a list of pre-built `LockedUser` rows
/// through the public trait method so the fake store's invariants stay
/// consistent. Use inside `#[tokio::test]` rather than a sync builder.
///
/// Example:
///
/// ```no_run
/// # use synapse_storage::test_mocks::{shared_fake_user_store, seed_locked_users};
/// # use synapse_storage::LockedUser;
/// # async fn example() {
/// let store = shared_fake_user_store();
/// seed_locked_users(&store, vec![
///     LockedUser {
///         id: 1, user_id: "@bad:example.com".into(),
///         reason: Some("spam".into()), locked_by: "@admin:example.com".into(),
///         created_ts: 1_700_000_000_000, unlocked_ts: None, is_active: true,
///     },
/// ]).await;
/// # }
/// ```
pub async fn seed_locked_users(store: &FakeUserStore, users: Vec<crate::LockedUser>) {
    use crate::user::UserStore;
    for user in users {
        // FakeUserStore::lock_user is idempotent for active locks per its
        // own internal invariant, so re-applying seed rows is safe.
        let _ = store.lock_user(&user.user_id, user.reason.as_deref(), &user.locked_by, user.created_ts).await;
    }
}

// =============================================================================
// InMemory storage adapters — standalone test doubles (no DB required)
// =============================================================================
//
// These mirror the key methods of the production storage types and store data
// in `HashMap`/`Vec` behind `RwLock`. Tests construct them directly without
// needing a Postgres pool.
//
// Trait extraction (Phase 3) will enable `Arc<dyn Trait>` injection so that
// these can be swapped into services. Until then, use them directly in
// unit tests or via `MockSyncServiceDepsBuilder` in synapse-services.

use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;

use crate::admin_media::{
    encode_media_cursor, AdminMediaInfo, AdminMediaPage, AdminMediaQuotaSummary, AdminMediaStoreApi, MediaCursor,
};
use crate::audit::{
    encode_audit_event_cursor, AuditEvent, AuditEventCursor, AuditEventFilters, AuditEventStoreApi,
    CreateAuditEventRequest,
};
use crate::background_update::{
    BackgroundUpdate, BackgroundUpdateHistory, BackgroundUpdateStats, BackgroundUpdateStoreApi,
    CreateBackgroundUpdateRequest,
};
#[cfg(feature = "cas-sso")]
use crate::cas::{
    CasProxyGrantingTicket, CasProxyTicket, CasRegisteredService, CasSloSession, CasStoreApi, CasTicket,
    CasUserAttribute, CreatePgtRequest, CreateProxyTicketRequest, CreateTicketRequest, RegisterServiceRequest,
};
use crate::rate_limit::RateLimitStoreApi;
use crate::room_tag::RoomTagStoreApi;
use crate::threepid::{CreateThreepidRequest, ThreepidStoreApi, ThreepidValidationSession, UserThreepid};
use synapse_common::ApiError;

// ── Sub-modules ──

/// The `access_token` module.
pub mod access_token;
/// The `account_data` module.
pub mod account_data;
/// The `admin_federation` module.
pub mod admin_federation;
/// The `admin_media` module.
pub mod admin_media;
/// The `audit_event` module.
pub mod audit_event;
/// The `background_update` module.
pub mod background_update;
/// The `cas` module.
#[cfg(feature = "cas-sso")]
pub mod cas;
/// The `dehydrated_device` module.
pub mod dehydrated_device;
/// The `device_list` module.
pub mod device_list;
/// The `directory` module.
pub mod directory;
/// The `event` module.
pub mod event;
/// The `filter` module.
pub mod filter;
/// The `member` module.
pub mod member;
/// The `oidc_user_mapping` module.
pub mod oidc_user_mapping;
/// The `openid_token` module.
pub mod openid_token;
/// The `presence` module.
pub mod presence;
/// The `push` module.
pub mod push;
/// The `quarantine_media` module.
pub mod quarantine_media;
/// The `rate_limit` module.
pub mod rate_limit;
/// The `refresh_token` module.
pub mod refresh_token;
/// The `registration_token` module.
pub mod registration_token;
/// The `relations` module.
pub mod relations;
/// The `room` module.
pub mod room;
/// The `room_account_data` module.
pub mod room_account_data;
/// The `room_summary` module.
pub mod room_summary;
/// The `room_tag` module.
pub mod room_tag;
/// The `sliding_sync` module.
pub mod sliding_sync;
/// The `space` module.
pub mod space;
/// The `thread` module.
pub mod thread;
/// The `threepid` module.
pub mod threepid;
/// The `worker` module.
pub mod worker;

// ── Re-exports ──

pub use access_token::InMemoryAccessTokenStore;
pub use account_data::InMemoryAccountDataStore;
pub use admin_federation::InMemoryAdminFederationStore;
pub use admin_media::InMemoryAdminMediaStore;
pub use audit_event::InMemoryAuditEventStore;
pub use background_update::InMemoryBackgroundUpdateStore;
#[cfg(feature = "cas-sso")]
pub use cas::InMemoryCasStore;
pub use dehydrated_device::InMemoryDehydratedDeviceStore;
pub use device_list::InMemoryDeviceListStore;
pub use directory::InMemoryDirectoryStore;
pub use event::InMemoryEventStore;
pub use filter::InMemoryFilterStore;
pub use member::InMemoryMemberStore;
pub use oidc_user_mapping::InMemoryOidcUserMappingStore;
pub use openid_token::InMemoryOpenIdTokenStore;
pub use presence::InMemoryPresenceStore;
pub use push::InMemoryPushStore;
pub use quarantine_media::InMemoryQuarantineMediaChangeStore;
pub use rate_limit::InMemoryRateLimitStore;
pub use refresh_token::InMemoryRefreshTokenStore;
pub use registration_token::InMemoryRegistrationTokenStore;
pub use relations::InMemoryRelationsStore;
pub use room::InMemoryRoomStore;
pub use room_account_data::InMemoryRoomAccountDataStore;
pub use room_summary::InMemoryRoomSummaryStore;
pub use room_tag::InMemoryRoomTagStore;
pub use sliding_sync::InMemorySlidingSyncStore;
pub use space::InMemorySpaceStore;
pub use thread::InMemoryThreadStore;
pub use threepid::InMemoryThreepidStore;
pub use worker::InMemoryWorkerStore;

#[cfg(test)]
mod tests;
