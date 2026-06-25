use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BlockRoomRequest {
    pub block: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MakeRoomAdminRequest {
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct BanRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoomUserActionRequest {
    pub user_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RoomTokenSyncQueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub from: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchRoomMessagesRequest {
    pub search_term: String,
    pub limit: Option<u32>,
    pub start_date: Option<i64>,
    pub end_date: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SearchAllRoomsRequest {
    pub search_term: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub from: Option<String>,
    pub order_by: Option<String>,
    pub is_public: Option<bool>,
    pub is_encrypted: Option<bool>,
}
