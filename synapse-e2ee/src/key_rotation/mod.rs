//! E2EE key rotation — the **megolm / Olm session** rotation subsystem.
//!
//! # What this module owns (M-05 model decision, §11.15)
//!
//! The `key_rotation*` table family is scoped **per user / device / room**:
//! `key_rotation_log(user_id, device_id, room_id, …)`,
//! `key_rotation_state(user_id, room_id)`,
//! `key_rotation_pending(room_id, triggered_by_user_id)`. It records **megolm
//! session rotation performed by the server** — the bridge / server-side-Megolm
//! case — and **not** federation signing-key rotation.
//!
//! Federation **signing key** rotation is a different domain with a different
//! table: `federation_signing_keys (server_name, key_id, …)`, owned by
//! `synapse-federation::key_rotation`. Upstream Synapse keeps that history in
//! `old_signing_keys` (`homeserver.yaml`) plus the `signing.key` file and
//! publishes it via `GET /_matrix/key/v2/server` → `old_verify_keys`; it has no
//! per-user rotation log at all.
//!
//! Grounds for the decision:
//!
//! - **Schema** — this log carries `user_id`/`device_id` (`NOT NULL`) and a
//!   nullable `room_id`; a per-server signing key has none of those.
//! - **Reader** — the only reader is device-scoped:
//!   `GET /_matrix/vendor/v1/keys/rotation/history/{device_id}`.
//! - **Siblings** — every `key_rotation*` table is user/room-scoped, and the
//!   only `rotation_type` ever written is `"megolm"`.
//! - **Spec** — Matrix puts megolm rotation on the *client*
//!   (`/#mroomencryption`: `rotation_period_msgs`, `rotation_period_ms`), so a
//!   server-side log of it is a vendor extension — hence `/_matrix/vendor/v1`.
//!
//! The machine-checked version of this boundary lives in
//! `service::db_tests::key_rotation_log_is_user_device_scoped_not_server_scoped`.

/// The `service` module.
pub mod service;

pub use service::KeyRotationConfig;
pub use service::KeyRotationLog;
pub use service::KeyRotationService;
pub use service::KeyRotationStorage;
pub use service::KeyRotationStorageApi;
pub use service::RotationStatus;
