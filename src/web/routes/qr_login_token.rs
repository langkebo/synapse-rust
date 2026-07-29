//! Short-lived login token store for MSC4108 QR sign-in.
//!
//! The existing device (already authenticated) generates a login token via
//! `POST /_matrix/client/v1/login/qr_token`. The token is sent to the new
//! device over the MSC4108 secure channel. The new device exchanges it for a
//! real access token via `POST /_matrix/client/v3/login` with
//! `type: "m.login.token"`.
//!
//! Tokens are single-use and expire after 60 seconds.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const LOGIN_TOKEN_TTL: Duration = Duration::from_secs(60);

struct LoginTokenEntry {
    user_id: String,
    device_id: Option<String>,
    expires_at: Instant,
    used: bool,
}

static LOGIN_TOKENS: LazyLock<Mutex<HashMap<String, LoginTokenEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Generate a new login token for the given user.
/// Returns the token string (a random UUID).
pub fn generate_login_token(user_id: &str, device_id: Option<&str>) -> String {
    let token = uuid::Uuid::new_v4().to_string();
    let entry = LoginTokenEntry {
        user_id: user_id.to_string(),
        device_id: device_id.map(|s| s.to_string()),
        expires_at: Instant::now() + LOGIN_TOKEN_TTL,
        used: false,
    };
    let mut map = LOGIN_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
    cleanup_expired(&mut map);
    map.insert(token.clone(), entry);
    token
}

/// Validate and consume a login token (single-use).
/// Returns `Some((user_id, device_id))` if valid, `None` otherwise.
pub fn consume_login_token(token: &str) -> Option<(String, Option<String>)> {
    let mut map = LOGIN_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
    cleanup_expired(&mut map);
    let entry = map.get_mut(token)?;
    if entry.used || entry.expires_at < Instant::now() {
        return None;
    }
    entry.used = true;
    let user_id = entry.user_id.clone();
    let device_id = entry.device_id.clone();
    // Remove consumed token
    map.remove(token);
    Some((user_id, device_id))
}

/// Remove expired tokens from the map.
fn cleanup_expired(map: &mut HashMap<String, LoginTokenEntry>) {
    let now = Instant::now();
    map.retain(|_, entry| entry.expires_at > now && !entry.used);
}
