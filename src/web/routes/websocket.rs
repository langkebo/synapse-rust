use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Clone)]
pub struct WebSocketManager {
    clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
    room_subscriptions: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

#[derive(Clone)]
struct ClientConnection {
    sender: broadcast::Sender<String>,
    rooms: Vec<String>,
    user_id: String,
}

impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            room_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn broadcast_room_event(&self, room_id: &str, event: serde_json::Value) {
        let payload = serde_json::json!({
            "type": "m.room.event",
            "room_id": room_id,
            "event": event
        });

        let msg = payload.to_string();
        let clients = self.clients.read().await;

        for (_, conn) in clients.iter() {
            if conn.rooms.contains(&room_id.to_string()) {
                let _ = conn.sender.send(msg.clone());
            }
        }
    }

    pub async fn broadcast_to_user(&self, user_id: &str, message: serde_json::Value) {
        let msg = message.to_string();
        let clients = self.clients.read().await;

        if let Some(conn) = clients.get(user_id) {
            let _ = conn.sender.send(msg);
        }
    }

    pub async fn broadcast_to_room(&self, room_id: &str, message: serde_json::Value) {
        let msg = message.to_string();
        let clients = self.clients.read().await;

        for (_, conn) in clients.iter() {
            if conn.rooms.contains(&room_id.to_string()) {
                let _ = conn.sender.send(msg.clone());
            }
        }
    }

    pub async fn register_client(&self, user_id: String) -> (broadcast::Receiver<String>, String) {
        let connection_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = broadcast::channel(100);

        let conn = ClientConnection {
            sender: tx,
            rooms: Vec::new(),
            user_id: user_id.clone(),
        };

        let mut clients = self.clients.write().await;
        clients.insert(user_id.clone(), conn);

        (rx, connection_id)
    }

    pub async fn subscribe_to_room(&self, user_id: &str, room_id: &str) -> bool {
        let mut clients = self.clients.write().await;

        if let Some(conn) = clients.get_mut(user_id) {
            if !conn.rooms.contains(&room_id.to_string()) {
                conn.rooms.push(room_id.to_string());
            }

            let mut subscriptions = self.room_subscriptions.write().await;
            subscriptions
                .entry(room_id.to_string())
                .or_insert_with(Vec::new)
                .push(user_id.to_string());

            return true;
        }
        false
    }

    pub async fn unsubscribe_from_room(&self, user_id: &str, room_id: &str) -> bool {
        let mut clients = self.clients.write().await;

        if let Some(conn) = clients.get_mut(user_id) {
            conn.rooms.retain(|r| r != room_id);

            let mut subscriptions = self.room_subscriptions.write().await;
            if let Some(subs) = subscriptions.get_mut(room_id) {
                subs.retain(|u| u != user_id);
            }

            return true;
        }
        false
    }

    pub async fn unregister_client(&self, user_id: &str) {
        let mut clients = self.clients.write().await;

        if let Some(conn) = clients.remove(user_id) {
            let mut subscriptions = self.room_subscriptions.write().await;
            for room_id in conn.rooms {
                if let Some(subs) = subscriptions.get_mut(&room_id) {
                    subs.retain(|u| u != user_id);
                }
            }
        }
    }

    pub async fn get_connected_users(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }

    pub async fn get_room_subscribers(&self, room_id: &str) -> Vec<String> {
        let subscriptions = self.room_subscriptions.read().await;
        subscriptions.get(room_id).cloned().unwrap_or_default()
    }

    pub async fn get_connection_count(&self) -> usize {
        self.clients.read().await.len()
    }

    pub async fn is_user_connected(&self, user_id: &str) -> bool {
        self.clients.read().await.contains_key(user_id)
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct WsQueryParams {
    pub user_id: Option<String>,
    pub access_token: Option<String>,
    pub room_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct WsMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: serde_json::Value,
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<WebSocketManager>>,
    Query(params): Query<WsQueryParams>,
) -> impl IntoResponse {
    let user_id = params.user_id.clone().unwrap_or_else(|| "anonymous".to_string());
    let room_id = params.room_id.clone();

    let resp = ws
        .protocols(["v1"])
        .on_upgrade(move |socket| handle_socket(socket, state, user_id, room_id));

    (
        [("Sec-WebSocket-Protocol", "v1")],
        resp,
    )
}

async fn handle_socket(
    socket: WebSocket,
    manager: Arc<WebSocketManager>,
    user_id: String,
    initial_room: Option<String>,
) {
    let (sender, mut receiver) = socket.split();
    let (tx, mut rx) = manager.register_client(user_id.clone()).await;

    if let Some(ref room) = initial_room {
        manager.subscribe_to_room(&user_id, room).await;

        let msg = WsMessage {
            msg_type: " subscribed".to_string(),
            data: serde_json::json!({ "room_id": room }),
        };
        if let Ok(json) = serde_json::to_string(&msg) {
            let _ = sender.send(Message::Text(json)).await;
        } else {
            tracing::warn!("Failed to serialize subscription message for user {}", user_id);
        }
    }

    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if sender.send(Message::Text(msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                msg = receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            tracing::debug!("Received from {}: {}", user_id, text);
                            if let Err(e) = handle_client_message(&manager, &user_id, &text).await {
                                tracing::warn!("Error handling client message: {}", e);
                            }
                        }
                        Some(Ok(Message::Close(frame))) => {
                            tracing::debug!("Client {} disconnected", user_id);
                            break;
                        }
                        Some(Err(e)) => {
                            tracing::warn!("WebSocket error for {}: {}", user_id, e);
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }
            }
        }
    }).await.ok();

    manager.unregister_client(&user_id).await;
}

async fn handle_client_message(
    manager: &Arc<WebSocketManager>,
    user_id: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let msg: serde_json::Value = serde_json::from_str(text)?;

    let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "subscribe" => {
            if let Some(room_id) = msg.get("room_id").and_then(|v| v.as_str()) {
                manager.subscribe_to_room(user_id, room_id).await;
                tracing::info!("User {} subscribed to room {}", user_id, room_id);
            }
        }
        "unsubscribe" => {
            if let Some(room_id) = msg.get("room_id").and_then(|v| v.as_str()) {
                manager.unsubscribe_from_room(user_id, room_id).await;
                tracing::info!("User {} unsubscribed from room {}", user_id, room_id);
            }
        }
        "ping" => {
            tracing::debug!("Received ping from {}", user_id);
        }
        _ => {
            tracing::debug!("Unknown message type from {}: {}", user_id, msg_type);
        }
    }

    Ok(())
}

pub fn websocket_routes() -> Router {
    Router::new().route("/ws", get(websocket_handler))
}

#[derive(Debug, serde::Serialize)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub rooms: Vec<RoomStats>,
}

#[derive(Debug, serde::Serialize)]
pub struct RoomStats {
    pub room_id: String,
    pub subscriber_count: usize,
}

pub async fn get_connection_stats(
    State(state): State<Arc<WebSocketManager>>,
) -> Json<ConnectionStats> {
    let total = state.get_connection_count().await;
    let subscriptions = state.room_subscriptions.read().await;

    let rooms: Vec<RoomStats> = subscriptions
        .iter()
        .map(|(room_id, users)| RoomStats {
            room_id: room_id.clone(),
            subscriber_count: users.len(),
        })
        .collect();

    Json(ConnectionStats {
        total_connections: total,
        rooms,
    })
}
