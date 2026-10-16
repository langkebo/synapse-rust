//! Worker self-announcement on the heartbeat (W-G).
//!
//! ## Why this exists
//!
//! A worker process could not get into the registry at all:
//!
//! * the only register route (`POST /_synapse/worker/v1/register`) sits behind
//!   **admin** auth (`admin_auth_middleware` → an admin access token), while a
//!   worker only holds the replication shared secret;
//! * the shared-secret surface has no register route;
//! * `WorkerStorage::update_worker_status` — what a heartbeat goes through — is
//!   `UPDATE workers … WHERE worker_id = $1`: **not an upsert, and it does not check
//!   rows affected**, so a heartbeat for an unknown worker returned 200, logged
//!   nothing, and changed zero rows.
//!
//! Consequence: in a real multi-process deployment the registry stayed empty, so
//! `runtime.missing` listed every declared instance, `get_active()` was empty, the
//! load balancer had no candidates, and task dispatch returned `None` for every
//! worker-only task type — while the in-memory `HealthChecker` (populated earlier in
//! the same handler) made health look fine.
//!
//! ## What this does
//!
//! The heartbeat already arrives on the shared-secret surface and already carries the
//! worker's `worker_id` in the path. This module lets it also carry the worker's
//! **identity**, and creates the registry row on first contact. No new or moved route
//! ⇒ the route contract chain is untouched, and the trust boundary is unchanged: that
//! secret already authorises commands, task claims, event reads and replication
//! position writes, so creating one's own registry row is not a new capability.
//!
//! Backward compatible by construction: a heartbeat WITHOUT the identity fields
//! behaves exactly as before (no row is created, nothing errors), which is what any
//! existing client sends.
//!
//! Residual risk worth knowing (not solved here): an announced name is taken at face
//! value, so a caller holding the shared secret can claim any instance name —
//! including a duty-bearing one such as `background_worker`, which would move global
//! maintenance ownership. Undeclared names surface as `unexpected` in
//! `GET /_synapse/worker/v1/topology/validate`; deciding who may claim what is the
//! "duty model" work (audit W-7), not this batch.

use std::str::FromStr;

use serde::Deserialize;
use tracing::{info, warn};

use synapse_common::ApiError;
use synapse_services::worker::types::{RegisterWorkerRequest, WorkerLoadStatsUpdate, WorkerType};

use crate::routes::context::AdminContext;

/// The `HeartbeatBody` struct.
///
/// Lives here rather than next to the other bodies so the identity fields and the
/// rule that consumes them stay together; re-exported from the parent module, so the
/// public path is unchanged.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeartbeatBody {
    /// The `status` field.
    pub status: String,
    /// The `load_stats` field.
    pub load_stats: Option<WorkerLoadStatsUpdate>,
    /// The instance name this worker is configured as (`worker.instance_name`).
    ///
    /// Optional: omitting it keeps the pre-W-G behaviour exactly.
    pub worker_name: Option<String>,
    /// The worker type this process is serving, in `WorkerType` spelling
    /// (`frontend`, `synchrotron`, …) — not the instance-name spelling.
    pub worker_type: Option<String>,
    /// Where this worker can be reached, if it knows. A `synapse_worker` process only
    /// serves `/metrics`, so it announces that listener.
    pub host: Option<String>,
    /// See [`HeartbeatBody::host`].
    pub port: Option<u16>,
}

/// What a heartbeat says about the worker sending it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnouncedIdentity {
    /// The instance name the worker claims (`worker.instance_map` keys are matched
    /// against this).
    pub worker_name: String,
    /// The type it serves.
    pub worker_type: WorkerType,
    /// Reachable host, when announced.
    pub host: Option<String>,
    /// Reachable port, when announced.
    pub port: Option<u16>,
}

impl HeartbeatBody {
    /// The identity this heartbeat announces, or `None` when it announces none.
    ///
    /// Both `worker_name` and `worker_type` are required to announce: a name without a
    /// type cannot build a registry row, and guessing one would invent facts. A blank
    /// name is treated as "not announced" rather than as an error — a worker with an
    /// unset environment variable should keep heartbeating, not start failing.
    ///
    /// An unparseable `worker_type` **is** an error (400): it means the caller tried to
    /// announce and got the spelling wrong, and silently ignoring it would look exactly
    /// like the silent-no-op behaviour this module exists to remove.
    pub fn announced_identity(&self) -> Result<Option<AnnouncedIdentity>, ApiError> {
        let (Some(name), Some(kind)) = (self.worker_name.as_deref(), self.worker_type.as_deref()) else {
            return Ok(None);
        };

        let worker_name = name.trim();
        if worker_name.is_empty() {
            return Ok(None);
        }

        let worker_type = WorkerType::from_str(kind).map_err(ApiError::bad_request)?;

        Ok(Some(AnnouncedIdentity {
            worker_name: worker_name.to_string(),
            worker_type,
            host: self.host.clone(),
            port: self.port,
        }))
    }
}

/// Create the registry row for `worker_id` on first contact.
///
/// Returns `true` when a row was created. Three cases are deliberately no-ops:
/// the worker is already known (the normal case — this runs on every beat), the
/// heartbeat announced no identity, or nothing needs writing. All three leave the
/// caller's behaviour byte-for-byte what it was before W-G.
pub async fn announce_if_unknown(ctx: &AdminContext, worker_id: &str, body: &HeartbeatBody) -> Result<bool, ApiError> {
    if ctx.worker_manager.get(worker_id).await?.is_some() {
        return Ok(false);
    }

    let Some(identity) = body.announced_identity()? else {
        // Not an error, but worth a line: without it the row can never appear and the
        // topology endpoint will keep reporting this instance as missing.
        warn!(
            worker_id,
            "Worker heartbeat carried no identity and the worker is unknown — its registry row cannot be created \
             (set worker.instance_name and have the worker announce it; see audit W-G)"
        );
        return Ok(false);
    };

    let host = identity.host.unwrap_or_else(|| "unknown".to_string());
    let port = identity.port.unwrap_or(0);
    let worker = ctx
        .worker_manager
        .register(RegisterWorkerRequest {
            worker_id: worker_id.to_string(),
            worker_name: identity.worker_name.clone(),
            worker_type: identity.worker_type,
            host,
            port,
            config: None,
            metadata: None,
            version: None,
        })
        .await?;

    info!(
        worker_id = %worker.worker_id,
        worker_name = %worker.worker_name,
        worker_type = %worker.worker_type.as_str(),
        "Worker registry row created from its heartbeat (W-G)"
    );

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(json: serde_json::Value) -> HeartbeatBody {
        serde_json::from_value(json).expect("fixture must deserialize into HeartbeatBody")
    }

    #[test]
    fn a_pre_w_g_heartbeat_still_deserializes_and_announces_nothing() {
        // Backward compatibility is the load-bearing property of this batch: every
        // existing worker sends `{status, load_stats?}` and must keep working
        // (heartbeat succeeds, no row is created, nothing errors).
        let body = body(serde_json::json!({ "status": "running" }));

        assert_eq!(body.announced_identity().expect("no identity is not an error"), None);
    }

    #[test]
    fn a_full_announcement_parses_into_the_identity_the_registry_needs() {
        let body = body(serde_json::json!({
            "status": "running",
            "worker_name": "event_persister",
            "worker_type": "event_persister",
            "host": "127.0.0.1",
            "port": 9091
        }));

        let identity = body.announced_identity().expect("parses").expect("announced");

        assert_eq!(identity.worker_name, "event_persister");
        assert_eq!(identity.worker_type, WorkerType::EventPersister);
        assert_eq!(identity.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(identity.port, Some(9091));
    }

    #[test]
    fn every_worker_type_spelling_the_worker_binary_sends_is_accepted() {
        // The worker sends `current_instance_worker_type(..).as_str()`, so these are the
        // actual spellings on the wire — not the instance-name ones (`sync_worker`,
        // `client_reader`), which would be rejected here.
        for (spelling, expected) in [
            ("master", WorkerType::Master),
            ("frontend", WorkerType::Frontend),
            ("background", WorkerType::Background),
            ("event_persister", WorkerType::EventPersister),
            ("synchrotron", WorkerType::Synchrotron),
            ("federation_sender", WorkerType::FederationSender),
            ("federation_reader", WorkerType::FederationReader),
            ("media_repository", WorkerType::MediaRepository),
            ("pusher", WorkerType::Pusher),
            ("appservice", WorkerType::AppService),
        ] {
            let body = body(serde_json::json!({
                "status": "running",
                "worker_name": "instance-1",
                "worker_type": spelling
            }));
            let identity = body.announced_identity().expect("parses").expect("announced");
            assert_eq!(identity.worker_type, expected, "spelling '{spelling}' must map to {expected:?}");
        }
    }

    #[test]
    fn an_unparseable_worker_type_is_rejected_instead_of_ignored() {
        // Silently ignoring it would reproduce exactly the failure this batch removes:
        // a worker that looks like it announced itself while no row can be created.
        let body = body(serde_json::json!({
            "status": "running",
            "worker_name": "sync_worker",
            "worker_type": "sync_worker"
        }));

        let error = body.announced_identity().expect_err("the instance-name spelling is not a worker type");

        assert_eq!(error.kind.default_http_status(), 400, "a bad announcement is a client error, got {error:?}");
        assert!(
            error.message.contains("Invalid worker type"),
            "the parse error must reach the caller, got: {}",
            error.message
        );
    }

    #[test]
    fn a_blank_name_or_a_missing_type_announces_nothing_rather_than_failing() {
        // Half an announcement is not a request to register; it is a deployment that has
        // not set the variable. Keep beating.
        for json in [
            serde_json::json!({ "status": "running", "worker_name": "   ", "worker_type": "frontend" }),
            serde_json::json!({ "status": "running", "worker_name": "client_reader" }),
            serde_json::json!({ "status": "running", "worker_type": "frontend" }),
        ] {
            let body = body(json.clone());
            assert_eq!(body.announced_identity().expect("not an error"), None, "fixture: {json}");
        }
    }

    #[test]
    fn the_announced_name_is_trimmed_and_host_port_are_optional() {
        let body = body(serde_json::json!({
            "status": "running",
            "worker_name": "  sync_worker  ",
            "worker_type": "synchrotron"
        }));

        let identity = body.announced_identity().expect("parses").expect("announced");

        assert_eq!(identity.worker_name, "sync_worker", "whitespace must not become part of the instance name");
        assert_eq!(identity.host, None);
        assert_eq!(identity.port, None);
    }

    #[test]
    fn unknown_fields_are_still_rejected() {
        // The body keeps `deny_unknown_fields`; adding identity fields must not turn the
        // endpoint into a sink for arbitrary JSON.
        let result = serde_json::from_value::<HeartbeatBody>(serde_json::json!({
            "status": "running",
            "worker_id": "not-in-the-body"
        }));

        assert!(result.is_err(), "an unknown field must still fail deserialization");
    }
}
