// ROUND2-ISSUE-1: test code may use unwrap/expect/unwrap_err/panic per Rust testing idiom.
// Production lib code is still held to the strict clippy lint config in [lints.clippy].
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
// B-3.1-b-3: synapse-e2ee fully documented + deny(missing_docs).
// ratchet baseline is tracked in scripts/quality/check_missing_docs_ratchet.sh;
// this crate is now at zero missing-doc warnings under `cargo doc`.
#![deny(missing_docs)]
//! End-to-end encryption for the synapse-rust Matrix homeserver.
//!
//! Implements Matrix E2EE: Olm (double ratchet) account/session management,
//! Megolm (group ratchet) outbound sessions, server-side encrypted key
//! backup, SSSS secret storage, cross-signing keys (MSC1756), and
//! interactive device verification (SAS / QR-code / numeric per
//! MSC2246/MSC3326). Public surface is split into per-feature modules
//! (e.g. `olm`, `megolm`, `backup`, `ssss`); consumers normally reach
//! them through the high-level `*Service` types re-exported below.

/// The `backup` module.
pub mod backup;
/// The `cross_signing` module.
pub mod cross_signing;
/// The `crypto` module.
pub mod crypto;
/// The `device_keys` module.
pub mod device_keys;
/// The `device_trust` module.
pub mod device_trust;
/// The `key_request` module.
pub mod key_request;
/// The `key_rotation` module.
pub mod key_rotation;
/// The `megolm` module.
pub mod megolm;
/// The `olm` module.
pub mod olm;
/// The `secure_backup` module.
pub mod secure_backup;
/// The `signature` module.
pub mod signature;
/// The `signed_json` module.
pub mod signed_json;
/// The `ssss` module.
pub mod ssss;
#[cfg(any(test, feature = "test-utils"))]
/// The `test_mocks` module.
pub mod test_mocks;
/// The `to_device` module.
pub mod to_device;
/// The `vodozemac_megolm` module.
pub mod vodozemac_megolm;

// 跨客户端互操作测试（Phase 3）。所有 case 需 `E2EE_INTEROP=1` 显式启用，
// 不会拖慢默认 `cargo test` 速度；本地 Element 互操作通过
// `.github/workflows/e2ee-interop.yml` 跑。
#[cfg(test)]
mod vodozemac_interop_tests;

// Explicit exports for backup module
pub use backup::models::{
    BackupKeyInfo, BackupKeyUpload, BackupKeyUploadRequest, BackupUploadRequest, BackupUploadResponse,
    BackupVerificationRequest, BackupVerificationResponse, BackupVersion, BatchRecoveryRequest, BatchRecoveryResponse,
    KeyBackup, RecoveryProgress, RecoveryRequest, RecoveryResponse, RecoverySession,
};
pub use backup::service::KeyBackupService;

// Explicit exports for secure_backup (Phase 3, client-side encryption only)
pub use secure_backup::models::{
    BackupVersion as SecureBackupVersion, EncryptedSessionKey, RestoreResponse, RestoreSecureBackupRequest,
    SecureBackupAuthData, SecureBackupInfo, SecureBackupResponse, SessionKeyData,
};
pub use secure_backup::service::SecureBackupService;
// Explicit exports to avoid ambiguous glob re-exports
pub use cross_signing::models::CrossSigningKey;
pub use cross_signing::models::CrossSigningKeys;
pub use cross_signing::models::DeviceKeyVerificationResult;
pub use cross_signing::models::VerifiedDevicesMap;
pub use cross_signing::service::CrossSigningService;
pub use cross_signing::storage::CrossSigningStorage;
pub use device_keys::models::*;
pub use device_keys::service::DeviceKeyService;
// Explicit exports for device_trust
pub use device_trust::models::{
    DeviceTrustLevel, DeviceTrustStatus, DeviceVerificationRequest, E2eeSecurityEvent, KeyRotationLog, SecuritySummary,
    VerificationMethod, VerificationRequestStatus,
};
pub use device_trust::service::DeviceTrustService;
pub use device_trust::storage::DeviceTrustStorage;
pub use key_request::{KeyRequestInfo, KeyRequestService};
pub use megolm::models::{EncryptedEvent, MegolmSession};
pub use megolm::service::MegolmProvider;
pub use olm::models::*;
pub use olm::OlmService;
pub use signature::EventSignature;
pub use signature::SignatureService;
pub use ssss::SecretStorage;
pub use ssss::SecretStorageService;
pub use verification::{
    QrCodeData, QrState, SasData, SasRepresentation, SasResult, SasState, VerificationMethod as VerifMethod,
    VerificationState,
};

/// The `verification` module.
pub mod verification;
