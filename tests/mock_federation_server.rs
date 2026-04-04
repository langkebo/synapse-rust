use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, put},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock Federation Server - 模拟远程 homeserver 的行为
/// 用于测试 Federation 互操作，无需真实的多实例部署
pub struct MockFederationServer {
    pub server_name: String,
    pub port: u16,
    // 存储接收到的事件
    received_events: Arc<Mutex<Vec<Value>>>,
    // 存储接收到的邀请
    received_invites: Arc<Mutex<HashMap<String, Vec<String>>>>,
    // 房间状态
    room_states: Arc<Mutex<HashMap<String, Value>>>,
}

impl MockFederationServer {
    pub fn new(server_name: &str, port: u16) -> Self {
        Self {
            server_name: server_name.to_string(),
            port,
            received_events: Arc::new(Mutex::new(Vec::new())),
            received_invites: Arc::new(Mutex::new(HashMap::new())),
            room_states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 创建 Axum Router
    pub fn create_router(&self) -> Router {
        let state = MockFederationState {
            server_name: self.server_name.clone(),
            received_events: self.received_events.clone(),
            received_invites: self.received_invites.clone(),
            room_states: self.room_states.clone(),
        };

        Router::new()
            .route("/_matrix/federation/v1/version", get(handle_version))
            .route("/_matrix/key/v2/server", get(handle_server_keys))
            .route(
                "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
                get(handle_make_join),
            )
            .route(
                "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
                put(handle_send_join),
            )
            .route(
                "/_matrix/federation/v1/invite/{room_id}/{event_id}",
                put(handle_invite),
            )
            .route(
                "/_matrix/federation/v1/send/{txn_id}",
                put(handle_send_transaction),
            )
            .route(
                "/_matrix/federation/v1/state/{room_id}",
                get(handle_get_state),
            )
            .with_state(state)
    }

    /// 检查是否收到特定房间的邀请
    pub fn received_invite(&self, room_id: &str, user_id: &str) -> bool {
        let invites = self.received_invites.lock().unwrap();
        invites
            .get(room_id)
            .map(|users| users.contains(&user_id.to_string()))
            .unwrap_or(false)
    }

    /// 获取接收到的事件数量
    pub fn received_events_count(&self) -> usize {
        self.received_events.lock().unwrap().len()
    }

    /// 获取房间的事件
    pub fn get_room_events(&self, room_id: &str) -> Vec<Value> {
        self.received_events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.get("room_id").and_then(|v| v.as_str()) == Some(room_id))
            .cloned()
            .collect()
    }

    /// 清空接收到的数据（用于测试隔离）
    pub fn clear(&self) {
        self.received_events.lock().unwrap().clear();
        self.received_invites.lock().unwrap().clear();
        self.room_states.lock().unwrap().clear();
    }
}

#[derive(Clone)]
struct MockFederationState {
    server_name: String,
    received_events: Arc<Mutex<Vec<Value>>>,
    received_invites: Arc<Mutex<HashMap<String, Vec<String>>>>,
    room_states: Arc<Mutex<HashMap<String, Value>>>,
}

// Handler: /_matrix/federation/v1/version
async fn handle_version(State(_state): State<MockFederationState>) -> impl IntoResponse {
    Json(json!({
        "server": {
            "name": "Mock Synapse",
            "version": "1.0.0-mock"
        }
    }))
}

// Handler: /_matrix/key/v2/server
async fn handle_server_keys(State(state): State<MockFederationState>) -> impl IntoResponse {
    Json(json!({
        "server_name": state.server_name,
        "verify_keys": {
            "ed25519:mock_key": {
                "key": "mock_verify_key_base64"
            }
        },
        "old_verify_keys": {},
        "valid_until_ts": 9999999999999i64
    }))
}

// Handler: /_matrix/federation/v1/make_join/:room_id/:user_id
async fn handle_make_join(
    State(state): State<MockFederationState>,
    Path((room_id, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    Json(json!({
        "event": {
            "type": "m.room.member",
            "room_id": room_id,
            "sender": user_id,
            "state_key": user_id,
            "content": {
                "membership": "join"
            },
            "origin": state.server_name,
            "origin_server_ts": chrono::Utc::now().timestamp_millis(),
            "unsigned": {}
        },
        "room_version": "10"
    }))
}

// Handler: /_matrix/federation/v1/send_join/{room_id}/{event_id}
async fn handle_send_join(
    State(state): State<MockFederationState>,
    Path((_room_id, _event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // 记录加入事件
    state.received_events.lock().unwrap().push(body.clone());

    Json(json!({
        "origin": state.server_name,
        "auth_chain": [],
        "state": [],
        "event": body
    }))
}

// Handler: /_matrix/federation/v1/invite/{room_id}/{event_id}
async fn handle_invite(
    State(state): State<MockFederationState>,
    Path((room_id, _event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // 记录邀请
    if let Some(state_key) = body.get("state_key").and_then(|v| v.as_str()) {
        state
            .received_invites
            .lock()
            .unwrap()
            .entry(room_id.clone())
            .or_insert_with(Vec::new)
            .push(state_key.to_string());
    }

    state.received_events.lock().unwrap().push(body.clone());

    Json(json!({
        "event": body
    }))
}

// Handler: /_matrix/federation/v1/send/{txn_id}
async fn handle_send_transaction(
    State(state): State<MockFederationState>,
    Path(_txn_id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // 记录事务中的所有事件
    if let Some(pdus) = body.get("pdus").and_then(|v| v.as_array()) {
        let mut events = state.received_events.lock().unwrap();
        for pdu in pdus {
            events.push(pdu.clone());
        }
    }

    Json(json!({
        "pdus": {}
    }))
}

// Handler: /_matrix/federation/v1/state/:room_id
async fn handle_get_state(
    State(state): State<MockFederationState>,
    Path(room_id): Path<String>,
) -> impl IntoResponse {
    let states = state.room_states.lock().unwrap();
    let room_state = states.get(&room_id).cloned().unwrap_or_else(|| {
        json!({
            "auth_chain": [],
            "pdus": []
        })
    });

    Json(room_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_server_creation() {
        let server = MockFederationServer::new("remote.test", 8009);
        assert_eq!(server.server_name, "remote.test");
        assert_eq!(server.port, 8009);
        assert_eq!(server.received_events_count(), 0);
    }

    #[test]
    fn test_mock_server_record_invite() {
        let server = MockFederationServer::new("remote.test", 8009);

        // 模拟记录邀请
        server
            .received_invites
            .lock()
            .unwrap()
            .entry("!room:test".to_string())
            .or_insert_with(Vec::new)
            .push("@user:remote.test".to_string());

        assert!(server.received_invite("!room:test", "@user:remote.test"));
        assert!(!server.received_invite("!room:test", "@other:remote.test"));
    }

    #[test]
    fn test_mock_server_clear() {
        let server = MockFederationServer::new("remote.test", 8009);

        // 添加一些数据
        server.received_events.lock().unwrap().push(json!({}));
        server
            .received_invites
            .lock()
            .unwrap()
            .insert("!room:test".to_string(), vec!["@user:test".to_string()]);

        assert_eq!(server.received_events_count(), 1);

        // 清空
        server.clear();
        assert_eq!(server.received_events_count(), 0);
        assert!(!server.received_invite("!room:test", "@user:test"));
    }
}
