// Event broadcaster trait unit tests — verifies the trait contract in
// `synapse_services::event_broadcaster_trait` (re-exported from
// `synapse_common::traits`).
//
// The module is a pure trait re-export. The trait `EventBroadcaster` is
// generic over an associated `Message` type and exposes:
//   * `broadcast_publish(message) -> Result<(), BroadcastError>`
//   * `broadcast_subscriber_count() -> usize`
//
// These tests verify:
//   * The trait can be implemented by a custom in-memory mock.
//   * `broadcast_publish` returns `Ok(())` on success and `Err(...)` on failure.
//   * `broadcast_subscriber_count` reflects the actual number of subscribers.
//   * `BroadcastError` variants format correctly via `Display`.
//   * The `From<serde_json::Error>` conversion produces `EncodingFailed`.
//   * Multiple implementations with different `Message` types satisfy the trait.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use synapse_common::traits::{BroadcastError, EventBroadcaster};

// ─────────────────────────────────────────────────────────────────────────────
// Mock implementations
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory broadcaster that counts subscribers and tracks the last
/// published message. Publish succeeds unless `fail_next` is set.
struct CountingBroadcaster {
    subscribers: AtomicUsize,
    last_message: std::sync::Mutex<Option<String>>,
    fail_next: std::sync::atomic::AtomicBool,
}

impl CountingBroadcaster {
    fn new(subscriber_count: usize) -> Self {
        Self {
            subscribers: AtomicUsize::new(subscriber_count),
            last_message: std::sync::Mutex::new(None),
            fail_next: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn set_fail_next(&self, fail: bool) {
        self.fail_next.store(fail, Ordering::SeqCst);
    }

    fn last_message(&self) -> Option<String> {
        self.last_message.lock().unwrap().clone()
    }
}

impl EventBroadcaster for CountingBroadcaster {
    type Message = String;

    fn broadcast_publish(&self, message: Self::Message) -> impl std::future::Future<Output = Result<(), BroadcastError>> + Send {
        let fail = self.fail_next.load(Ordering::SeqCst);
        // `std::sync::Mutex` is NOT reentrant: acquiring the lock twice on the
        // same mutex (as the previous code did) deadlocks forever. Take the
        // lock exactly once inside a scoped block so it releases before the
        // async block is returned.
        {
            let mut last = self.last_message.lock().unwrap();
            *last = Some(message);
        }
        async move {
            if fail {
                Err(BroadcastError::ChannelFull("mock forced failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    fn broadcast_subscriber_count(&self) -> usize {
        self.subscribers.load(Ordering::SeqCst)
    }
}

/// A second implementation with a different Message type (structured JSON)
/// to verify the trait supports heterogeneous message payloads.
struct JsonBroadcaster {
    subscriber_count: usize,
    published: std::sync::Mutex<Vec<serde_json::Value>>,
}

impl JsonBroadcaster {
    fn new() -> Self {
        Self { subscriber_count: 3, published: std::sync::Mutex::new(Vec::new()) }
    }

    fn published_count(&self) -> usize {
        self.published.lock().unwrap().len()
    }
}

impl EventBroadcaster for JsonBroadcaster {
    type Message = serde_json::Value;

    fn broadcast_publish(&self, message: Self::Message) -> impl std::future::Future<Output = Result<(), BroadcastError>> + Send {
        self.published.lock().unwrap().push(message);
        async { Ok(()) }
    }

    fn broadcast_subscriber_count(&self) -> usize {
        self.subscriber_count
    }
}

/// Broadcaster that always returns `NotConnected` to exercise the error path.
struct DisconnectedBroadcaster;

impl EventBroadcaster for DisconnectedBroadcaster {
    type Message = String;

    fn broadcast_publish(&self, _message: Self::Message) -> impl std::future::Future<Output = Result<(), BroadcastError>> + Send {
        async { Err(BroadcastError::NotConnected) }
    }

    fn broadcast_subscriber_count(&self) -> usize {
        0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trait contract — subscriber_count
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn counting_broadcaster_reports_initial_subscriber_count() {
    let broadcaster = CountingBroadcaster::new(5);
    assert_eq!(broadcaster.broadcast_subscriber_count(), 5);
}

#[test]
fn counting_broadcaster_reports_zero_subscribers() {
    let broadcaster = CountingBroadcaster::new(0);
    assert_eq!(broadcaster.broadcast_subscriber_count(), 0);
}

#[test]
fn json_broadcaster_reports_subscriber_count() {
    let broadcaster = JsonBroadcaster::new();
    assert_eq!(broadcaster.broadcast_subscriber_count(), 3);
}

#[test]
fn disconnected_broadcaster_reports_zero_subscribers() {
    let broadcaster = DisconnectedBroadcaster;
    assert_eq!(broadcaster.broadcast_subscriber_count(), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Trait contract — broadcast_publish (success path)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn counting_broadcaster_publish_succeeds_when_not_failing() {
    let broadcaster = CountingBroadcaster::new(1);
    let result = broadcaster.broadcast_publish("hello".to_string()).await;
    assert!(result.is_ok());
    assert_eq!(broadcaster.last_message().as_deref(), Some("hello"));
}

#[tokio::test]
async fn json_broadcaster_publish_stores_message() {
    let broadcaster = JsonBroadcaster::new();
    let msg = serde_json::json!({"event": "m.room.message", "body": "hi"});
    let result = broadcaster.broadcast_publish(msg).await;
    assert!(result.is_ok());
    assert_eq!(broadcaster.published_count(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Trait contract — broadcast_publish (error path)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn counting_broadcaster_publish_returns_channel_full_when_fail_next_set() {
    let broadcaster = CountingBroadcaster::new(1);
    broadcaster.set_fail_next(true);
    let result = broadcaster.broadcast_publish("hello".to_string()).await;
    assert!(result.is_err());
    match result {
        Err(BroadcastError::ChannelFull(_)) => {}
        Err(other) => panic!("expected ChannelFull, got {other:?}"),
        Ok(_) => panic!("expected error, got Ok"),
    }
}

#[tokio::test]
async fn disconnected_broadcaster_publish_returns_not_connected() {
    let broadcaster = DisconnectedBroadcaster;
    let result = broadcaster.broadcast_publish("hello".to_string()).await;
    assert!(matches!(result, Err(BroadcastError::NotConnected)));
}

// ─────────────────────────────────────────────────────────────────────────────
// BroadcastError — Display formatting
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn broadcast_error_not_connected_display() {
    let err = BroadcastError::NotConnected;
    let s = format!("{err}");
    assert_eq!(s, "Not connected");
}

#[test]
fn broadcast_error_encoding_failed_display() {
    let err = BroadcastError::EncodingFailed("invalid utf8".to_string());
    let s = format!("{err}");
    assert!(s.contains("invalid utf8"));
    assert!(s.contains("Encoding failed"));
}

#[test]
fn broadcast_error_transport_display() {
    let err = BroadcastError::Transport("connection reset".to_string());
    let s = format!("{err}");
    assert!(s.contains("connection reset"));
    assert!(s.contains("Transport error"));
}

#[test]
fn broadcast_error_channel_full_display() {
    let err = BroadcastError::ChannelFull("queue depth 1000".to_string());
    let s = format!("{err}");
    assert!(s.contains("queue depth 1000"));
    assert!(s.contains("Channel full"));
}

#[test]
fn broadcast_error_other_display() {
    let err = BroadcastError::Other("custom reason".to_string());
    let s = format!("{err}");
    assert_eq!(s, "custom reason");
}

// ─────────────────────────────────────────────────────────────────────────────
// BroadcastError — From<serde_json::Error>
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn broadcast_error_from_serde_json_error_produces_encoding_failed() {
    // Trigger a real serde_json::Error by parsing invalid JSON.
    let json_err = serde_json::from_str::<serde_json::Value>("{invalid}").expect_err("must fail to parse");
    let broadcast_err: BroadcastError = json_err.into();
    match broadcast_err {
        BroadcastError::EncodingFailed(_) => {}
        other => panic!("expected EncodingFailed, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trait object — multiple implementations behave differently
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn different_implementations_exhibit_different_behaviors() {
    let counting = CountingBroadcaster::new(2);
    let disconnected = DisconnectedBroadcaster;

    // Counting succeeds; disconnected fails.
    assert!(counting.broadcast_publish("a".to_string()).await.is_ok());
    assert!(disconnected.broadcast_publish("a".to_string()).await.is_err());

    // Different subscriber counts.
    assert_eq!(counting.broadcast_subscriber_count(), 2);
    assert_eq!(disconnected.broadcast_subscriber_count(), 0);
}

#[tokio::test]
async fn counting_broadcaster_can_be_wrapped_in_arc_and_shared() {
    // Verify the trait is object-safe enough to be used through Arc.
    let broadcaster = Arc::new(CountingBroadcaster::new(1));
    let cloned = broadcaster.clone();

    let result = cloned.broadcast_publish("shared".to_string()).await;
    assert!(result.is_ok());
    assert_eq!(broadcaster.last_message().as_deref(), Some("shared"));
}
