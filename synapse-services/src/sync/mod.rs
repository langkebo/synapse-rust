//! Sync services domain group.
//!
//! Re-exports sync-related service modules (sync_service, sliding_sync_service,
//! sync_helpers) under a single namespace so that new sync modules can be added
//! here without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_services::sync::SyncService` over the
//! flat `synapse_services::SyncService`.

pub use crate::sliding_sync_service::SlidingSyncService;
pub use crate::sync_helpers::{room_event_to_json, state_event_to_json};
pub use crate::sync_service::{
    BuildRoomSyncRequest, BuildRoomSyncValueRequest, BuildSyncResponseRequest, FetchEventsRequest, IncrementalUpdate,
    LazyLoadMembersRequest, LazyLoadedMembersCacheKey, RoomFilter, RoomSyncCounts, RoomSyncState,
    StateEventsBatchParams, SyncEventFormat, SyncFilter, SyncPerformanceSnapshot, SyncRequest, SyncResponseFilter,
    SyncRoomSection, SyncService, SyncServiceApi, SyncServiceDeps, SyncServiceRequest, SyncState, SyncToken,
};
