//! Sync storage domain group.
//!
//! Re-exports sync-related storage modules (`sliding_sync`, `search_index`)
//! under a single namespace so that new sync storage modules can be added here
//! without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::sync::SlidingSyncStorage` over
//! the flat `synapse_storage::SlidingSyncStorage`.

pub use crate::search_index::{
    SearchIndexCursor, SearchIndexEntry, SearchIndexStats, SearchIndexStorage, SearchIndexStoreApi, SearchQuery,
    SearchResult,
};
pub use crate::sliding_sync::{
    decode_room_token_sync_cursor, encode_room_token_sync_cursor, AdminRoomTokenSyncEntry, RoomTokenSyncCursor,
    SlidingSyncFilters, SlidingSyncList, SlidingSyncListData, SlidingSyncListQuery, SlidingSyncListRequest,
    SlidingSyncRequest, SlidingSyncResponse, SlidingSyncRoom, SlidingSyncStorage, SlidingSyncStoreApi,
    SlidingSyncToken,
};

// P7.3: filter and presence are sync-related storage modules — group them
// under `sync::` so they are flat-re-exported via `pub use sync::*;` rather
// than via explicit flat re-exports in lib.rs.
pub use crate::filter::{CreateFilterRequest, Filter, FilterStorage, FilterStoreApi};
pub use crate::presence::{PresenceSnapshot, PresenceStorage};
