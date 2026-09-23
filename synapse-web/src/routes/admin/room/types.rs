use serde::Deserialize;

/// The `BlockRoomRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockRoomRequest {
    /// The `block` field.
    pub block: bool,
    /// The `reason` field.
    pub reason: Option<String>,
}

/// The `MakeRoomAdminRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MakeRoomAdminRequest {
    /// The `user_id` field.
    pub user_id: String,
}

/// The `BanRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanRequest {
    /// The `reason` field.
    pub reason: Option<String>,
}

/// The `RoomUserActionRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomUserActionRequest {
    /// The `user_id` field.
    pub user_id: String,
    /// The `reason` field.
    pub reason: Option<String>,
}

/// The `RoomTokenSyncQueryParams` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomTokenSyncQueryParams {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `offset` field.
    pub offset: Option<i64>,
    /// The `from` field.
    pub from: Option<String>,
}

/// The `SearchRoomMessagesRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRoomMessagesRequest {
    /// The `search_term` field.
    pub search_term: String,
    /// The `limit` field.
    pub limit: Option<u32>,
    /// The `start_date` field.
    pub start_date: Option<i64>,
    /// The `end_date` field.
    pub end_date: Option<i64>,
}

/// The `SearchAllRoomsRequest` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchAllRoomsRequest {
    /// The `search_term` field.
    pub search_term: Option<String>,
    /// The `limit` field.
    pub limit: Option<u32>,
    /// The `offset` field.
    pub offset: Option<u32>,
    /// The `from` field.
    pub from: Option<String>,
    /// The `order_by` field.
    pub order_by: Option<String>,
    /// The `is_public` field.
    pub is_public: Option<bool>,
    /// The `is_encrypted` field.
    pub is_encrypted: Option<bool>,
}

/// MSC3912: Request body for cascade redaction.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CascadeRedactRequest {
    /// Maximum recursion depth for cascade redaction (default 5).
    pub max_depth: Option<u32>,
}
