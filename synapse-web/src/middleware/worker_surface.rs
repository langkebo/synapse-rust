//! The worker HTTP surface gate (W-F).
//!
//! Installed by the server assembly (`synapse_rust::server::router::build_router`)
//! when `worker.surface_gate` is on **and** this process is not the master. It
//! answers 404 `M_NOT_FOUND` for any path the instance's role does not own, so
//! that behind a reverse proxy "who serves `/sync`" is an enforced fact rather
//! than a comment in a yaml file.
//!
//! Why a middleware and not a filtered router: the route table is generated and
//! contract-checked (`scripts/contract/`). Rebuilding it per role would fork that
//! surface into N variants, and a route registered in only one of them is exactly
//! the class of bug the contract chain exists to prevent. The gate leaves the
//! table intact and narrows what an instance *answers*.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use synapse_common::error::ApiError;
use synapse_services::worker::surface_gate::role_owns_path;
use synapse_services::worker::types::WorkerType;

/// The role the gate enforces.
#[derive(Debug, Clone, Copy)]
pub struct WorkerSurfaceGate {
    /// This instance's worker type.
    pub role: WorkerType,
}

impl WorkerSurfaceGate {
    /// Build a gate for `role`.
    pub fn new(role: WorkerType) -> Self {
        Self { role }
    }
}

/// Reject requests for paths this instance's role does not own.
///
/// The refusal is a plain `404 M_NOT_FOUND`, not a 403: an instance that does not
/// own a prefix must be indistinguishable from one where the route does not
/// exist, otherwise the surface itself becomes a directory of the deployment.
/// The role and path go to the log at `debug` instead, which is where an operator
/// debugging a proxy rule will look.
pub async fn worker_surface_middleware(
    State(gate): State<WorkerSurfaceGate>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if role_owns_path(gate.role, path) {
        return next.run(request).await;
    }

    ::tracing::debug!(
        worker_type = gate.role.as_str(),
        path = %path,
        method = %request.method(),
        "Worker surface gate refused a path this instance does not own \
         (worker.surface_gate is on; check the reverse-proxy rule)"
    );

    ApiError::not_found("Not found".to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn app_with_gate(role: WorkerType) -> Router {
        Router::new()
            .route("/_matrix/client/v3/sync", get(|| async { "sync" }))
            .route("/_matrix/media/v3/download/a/b", get(|| async { "media" }))
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(WorkerSurfaceGate::new(role), worker_surface_middleware))
    }

    async fn status_of(app: &Router, path: &str) -> (u16, String) {
        let request = Request::builder().uri(path).body(Body::empty()).unwrap();
        let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    #[tokio::test]
    async fn owned_paths_pass_and_unowned_paths_are_404() {
        let app = app_with_gate(WorkerType::Synchrotron);

        assert_eq!(status_of(&app, "/_matrix/client/v3/sync").await.0, 200, "sync is owned");

        let (status, body) = status_of(&app, "/_matrix/media/v3/download/a/b").await;
        assert_eq!(status, 404, "media is not owned by a synchrotron: {body}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body).unwrap()["errcode"],
            "M_NOT_FOUND",
            "the refusal must be a Matrix error, not an empty 404 body: {body}"
        );

        assert_eq!(status_of(&app, "/health").await.0, 200, "health is instance-agnostic");
    }

    #[tokio::test]
    async fn the_master_role_is_never_narrowed() {
        let app = app_with_gate(WorkerType::Master);

        // Every route the toy app has stays reachable for master, including the
        // one no master prefix table would list.
        for path in ["/_matrix/client/v3/sync", "/_matrix/media/v3/download/a/b", "/health"] {
            assert_eq!(status_of(&app, path).await.0, 200, "master must serve {path}");
        }
    }

    #[tokio::test]
    async fn a_role_with_no_prefixes_still_answers_infrastructure_paths() {
        let app = app_with_gate(WorkerType::Pusher);

        assert_eq!(status_of(&app, "/health").await.0, 200);
        assert_eq!(status_of(&app, "/_matrix/client/v3/sync").await.0, 404, "a pusher owns no client surface");
    }
}
