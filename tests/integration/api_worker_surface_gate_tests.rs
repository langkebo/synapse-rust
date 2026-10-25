//! W-F end-to-end: the surface gate is actually **installed** by the assembly.
//!
//! The unit tests prove the predicate (`role_owns_path`) and the middleware, and
//! the ledger guards prove the prefix table agrees with the real route table.
//! None of them proves the thing that can silently rot: that
//! `build_router` puts the layer on the router at all. A gate that is never
//! wired looks exactly like a working one from the outside — every path answers
//! normally — so this file pins the two directions:
//!
//! * `worker.surface_gate: true` on a non-master instance ⇒ unowned paths are 404;
//! * the *same* instance with the switch off ⇒ the same paths are **not** 404.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

/// Owned by a synchrotron (it is a `*/sync` route).
const SYNC: &str = "/_matrix/client/v3/sync";
/// A client route the synchrotron does **not** own.
const MEDIA_CONFIG: &str = "/_matrix/client/v3/media/config";
/// The admin surface: master-only by design.
const ADMIN_USERS: &str = "/_synapse/admin/v1/users";

async fn get(app: &axum::Router, path: &str) -> (StatusCode, Value) {
    let request = Request::builder().method("GET").uri(path).body(Body::empty()).unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| json_empty()))
}

fn json_empty() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Point the container's config at a narrowed synchrotron instance.
fn configure_instance(container: &mut synapse_services::ServiceContainer, instance: &str, surface_gate: bool) {
    let mut config = (*container.core.config).clone();
    config.worker.enabled = true;
    config.worker.instance_name = instance.to_string();
    config.worker.surface_gate = surface_gate;
    container.core.config = Arc::new(config);
}

#[tokio::test]
async fn a_narrowed_instance_serves_its_own_prefix_and_refuses_the_rest() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        configure_instance(container, "sync_worker-1", true);
    })
    .await
    else {
        super::skip_or_fail_without_db();
        return;
    };

    // Instance-agnostic: every instance answers capability discovery, otherwise a
    // healthy worker looks dead to a client that asks before syncing.
    let (status, body) = get(&app, "/_matrix/client/v3/versions").await;
    assert_eq!(status, StatusCode::OK, "versions is instance-agnostic, got {body}");

    // Owned: the gate lets it through, so the *auth* layer is what answers.
    let (status, body) = get(&app, SYNC).await;
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "a synchrotron must serve its own /sync route (got 404, so the gate refused it): {body}"
    );

    // Not owned: refused by the gate, with a Matrix error body.
    for path in [MEDIA_CONFIG, ADMIN_USERS] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path} must be refused for a synchrotron: {body}");
        assert_eq!(
            body["errcode"], "M_NOT_FOUND",
            "a refusal must look like a missing route, not an empty 404: {body}"
        );
    }
}

/// The control: same role, same instance name, switch off ⇒ nothing is narrowed.
///
/// Without this, the 404s above could just as well come from a path that the
/// router never served — the classic vacuous negative test.
#[tokio::test]
async fn without_the_switch_the_same_paths_are_not_refused() {
    let Some((app, _state)) = super::setup_fresh_test_app_with_config(|container| {
        configure_instance(container, "sync_worker-1", false);
    })
    .await
    else {
        super::skip_or_fail_without_db();
        return;
    };

    for path in [MEDIA_CONFIG, ADMIN_USERS] {
        let (status, body) = get(&app, path).await;
        assert_ne!(
            status,
            StatusCode::NOT_FOUND,
            "{path} must not be 404 when worker.surface_gate is off — otherwise the gate test is \
             asserting a missing route: {body}"
        );
    }

    // And the owned path is still reachable.
    let (status, body) = get(&app, SYNC).await;
    assert_ne!(status, StatusCode::NOT_FOUND, "{body}");
}
